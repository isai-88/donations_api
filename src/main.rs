use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, env, net::SocketAddr};
use reqwest::Client;

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

/// Respuesta de `/v2/users/{userId}/games`
#[derive(Deserialize)]
struct UserGamesResponse {
    data: Vec<UserGame>,
    #[serde(default)]
    nextPageCursor: Option<String>,
}

#[derive(Deserialize)]
struct UserGame {
    id: u64, // universeId
}

/// Respuesta de `/v2/games/{gameId}/game-passes`
#[derive(Deserialize)]
struct RobloxListResponse {
    data: Vec<RobloxPass>,
}

#[derive(Deserialize)]
struct RobloxPass {
    id: u64,
    name: String,
    #[allow(dead_code)]
    productId: Option<u64>,
}

/// Pide todos los juegos públicos de un usuario (paginando si hace falta)
async fn fetch_user_games(client: &Client, user_id: u64) -> Vec<u64> {
    let mut games = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!(
            "https://games.roblox.com/v2/users/{}/games?accessFilter=Public&limit=100&sortOrder=Asc",
            user_id
        );

        if let Some(c) = &cursor {
            url.push_str("&cursor=");
            url.push_str(c);
        }

        println!("[debug] GET {}", url);

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(err) => {
                println!("[warn] Error pidiendo lista de juegos: {:?}", err);
                break;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            println!(
                "[warn] Lista de juegos devolvió {}: {}",
                status, text
            );
            break;
        }

        let body: UserGamesResponse = match resp.json().await {
            Ok(b) => b,
            Err(err) => {
                println!("[warn] Error parseando JSON de juegos: {:?}", err);
                break;
            }
        };

        for g in &body.data {
            games.push(g.id);
        }

        if let Some(next) = body.nextPageCursor {
            cursor = Some(next);
        } else {
            break;
        }
    }

    println!(
        "[debug] Usuario {} tiene {} juegos públicos encontrados",
        user_id,
        games.len()
    );

    games
}

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let client = Client::new();

    // 1) Buscar todos los juegos del usuario
    let games = fetch_user_games(&client, user_id).await;

    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    // 2) Para cada juego, pedir sus gamepasses
    for game_id in games {
        let url = format!(
            "https://games.roblox.com/v2/games/{}/game-passes?limit=100&sortOrder=Asc",
            game_id
        );

        println!("[debug] Pidiendo gamepasses de juego {}", game_id);

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(err) => {
                println!(
                    "[warn] Error pidiendo gamepasses de juego {}: {:?}",
                    game_id, err
                );
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            println!(
                "[warn] Gamepasses de juego {} devolvieron {}: {}",
                game_id, status, text
            );
            continue;
        }

        let list: RobloxListResponse = match resp.json().await {
            Ok(l) => l,
            Err(err) => {
                println!(
                    "[warn] Error parseando JSON de gamepasses (juego {}): {:?}",
                    game_id, err
                );
                continue;
            }
        };

        println!(
            "[debug] Juego {} devolvió {} gamepasses",
            game_id,
            list.data.len()
        );

        for pass in list.data {
            if seen_ids.insert(pass.id) {
                // Si necesitas filtrar por creador o precio > 0, aquí es donde iría.
                result.push(Gamepass {
                    id: pass.id,
                    name: pass.name.clone(),
                    price: 0, // Si quieres luego podemos pedir los detalles para sacar el precio real
                });
            }
        }
    }

    println!(
        "[debug] Usuario {}: total de {} gamepasses agrupados",
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
    // Ruta principal
    let app = Router::new().route("/user/:id/passes", get(get_passes));

    // Puerto desde env.PORT o 8080 por defecto (Railway pone PORT)
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 Rust API escuchando en {addr}");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

