use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, env, net::SocketAddr};

#[derive(Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(rename = "userId")]
    user_id: u64,
    count: usize,
    passes: Vec<Gamepass>,
}

#[derive(Serialize, Clone)]
struct Gamepass {
    id: u64,
    name: String,
    price: i32,
}

/// Respuesta de /v2/users/{userId}/games (lista de juegos públicos)
#[derive(Deserialize)]
struct UserGamesResponse {
    data: Vec<UserGame>,
    #[serde(default)]
    nextPageCursor: Option<String>,
}

#[derive(Deserialize)]
struct UserGame {
    id: u64, // universeId del juego
}

/// Respuesta de /v2/games/{gameId}/game-passes
#[derive(Deserialize)]
struct RobloxPassList {
    data: Vec<RobloxPass>,
}

#[derive(Deserialize)]
struct RobloxPass {
    id: u64,
    name: String,
    #[allow(dead_code)]
    productId: Option<u64>,
}

/// Pide el PRIMER juego público del usuario (universeId)
async fn fetch_first_game_id(client: &Client, user_id: u64) -> Option<u64> {
    let url = format!(
        "https://games.roblox.com/v2/users/{}/games?accessFilter=Public&limit=1&sortOrder=Asc",
        user_id
    );

    println!("[debug] GET {}", url);

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(err) => {
            println!("[warn] Error pidiendo lista de juegos: {:?}", err);
            return None;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        println!(
            "[warn] Lista de juegos devolvió {}: {}",
            status, text
        );
        return None;
    }

    let body: UserGamesResponse = match resp.json().await {
        Ok(b) => b,
        Err(err) => {
            println!("[warn] Error parseando JSON de juegos: {:?}", err);
            return None;
        }
    };

    if let Some(first) = body.data.first() {
        println!(
            "[debug] Primer juego público de usuario {}: universeId={}",
            user_id, first.id
        );
        Some(first.id)
    } else {
        println!(
            "[debug] Usuario {} no tiene juegos públicos en la API.",
            user_id
        );
        None
    }
}

/// Handler: /user/:id/passes
async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let client = Client::new();

    // 1) Conseguir el PRIMER juego público del usuario
    let maybe_game_id = fetch_first_game_id(&client, user_id).await;

    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    let game_id = match maybe_game_id {
        Some(id) => id,
        None => {
            // Sin juegos → respuesta vacía pero ok=true
            return Json(ApiResponse {
                ok: true,
                user_id,
                count: 0,
                passes: vec![],
            });
        }
    };

    // 2) Pedir los gamepasses de ese juego
    let passes_url = format!(
        "https://games.roblox.com/v2/games/{}/game-passes?limit=100&sortOrder=Asc",
        game_id
    );

    println!(
        "[debug] Pidiendo gamepasses del juego (universeId) {}",
        game_id
    );

    let resp = match client.get(&passes_url).send().await {
        Ok(r) => r,
        Err(err) => {
            println!(
                "[warn] Error pidiendo gamepasses de juego {}: {:?}",
                game_id, err
            );
            return Json(ApiResponse {
                ok: true,
                user_id,
                count: 0,
                passes: vec![],
            });
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        println!(
            "[warn] Gamepasses de juego {} devolvieron {}: {}",
            game_id, status, text
        );
        return Json(ApiResponse {
            ok: true,
            user_id,
            count: 0,
            passes: vec![],
        });
    }

    let list: RobloxPassList = match resp.json().await {
        Ok(l) => l,
        Err(err) => {
            println!(
                "[warn] Error parseando JSON de gamepasses (juego {}): {:?}",
                game_id, err
            );
            return Json(ApiResponse {
                ok: true,
                user_id,
                count: 0,
                passes: vec![],
            });
        }
    };

    println!(
        "[debug] Juego {} devolvió {} gamepasses",
        game_id,
        list.data.len()
    );

    // 3) Para cada pase, pedimos detalles (creador, precio)
    for pass in list.data {
        if !seen_ids.insert(pass.id) {
            continue; // evitar duplicados
        }

        let detail_url = format!(
            "https://economy.roblox.com/v2/assets/{}/details",
            pass.id
        );

        if let Ok(detail_resp) = client.get(&detail_url).send().await {
            if !detail_resp.status().is_success() {
                continue;
            }

            if let Ok(details) = detail_resp.json::<Value>().await {
                let creator_id = details["Creator"]["Id"].as_u64().unwrap_or(0);

                // Solo los pases que REALMENTE creó este usuario
                if creator_id != user_id {
                    continue;
                }

                let price_i64 = details["PriceInRobux"]
                    .as_i64()
                    .or_else(|| details["Price"].as_i64())
                    .unwrap_or(0);

                let price = price_i64 as i32;

                // Saltar gratuitos
                if price <= 0 {
                    continue;
                }

                result.push(Gamepass {
                    id: pass.id,
                    name: pass.name.clone(),
                    price,
                });
            }
        }
    }

    println!(
        "[debug] Usuario {}: total de {} gamepasses filtrados en el primer juego",
        user_id,
        result.len()
    );

    Json(ApiResponse {
        ok: true,
        user_id,
        count: result.len(),
        passes: result,
    })
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/user/:id/passes", get(get_passes));

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0,], port));
    println!("🚀 Rust API escuchando en {addr}");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

