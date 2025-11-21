use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use serde::{Serialize, Deserialize};
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

// ---------- Helpers ----------

/// Intenta obtener gamepasses a partir de los **juegos públicos** del usuario.
/// 1) /v2/users/{userId}/games  → juegos públicos
/// 2) /v2/games/{universeId}/game-passes → passes del juego
/// 3) /v2/assets/{id}/details → precio
async fn fetch_passes_from_public_games(user_id: u64) -> Vec<Gamepass> {
    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    // 1) Juegos públicos del usuario
    let games_url = format!(
        "https://games.roblox.com/v2/users/{}/games?accessFilter=2&limit=50&sortOrder=Asc",
        user_id
    );
    println!("[API] Pidiendo juegos públicos para userId={} en {}", user_id, games_url);

    let games_resp = match reqwest::get(&games_url).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[API] Error HTTP al pedir juegos públicos: {e}");
            return result;
        }
    };

    if !games_resp.status().is_success() {
        eprintln!(
            "[API] Juegos públicos HTTP {} para userId={}",
            games_resp.status(),
            user_id
        );
        return result;
    }

    let games_json: serde_json::Value = match games_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[API] Error parseando JSON de juegos públicos: {e}");
            return result;
        }
    };

    let Some(games_arr) = games_json.get("data").and_then(|v| v.as_array()) else {
        println!("[API] Juegos públicos: no hay array 'data' para userId={}", user_id);
        return result;
    };

    let mut universe_ids: Vec<u64> = Vec::new();
    for game in games_arr {
        if let Some(id) = game.get("id").and_then(|v| v.as_u64()) {
            universe_ids.push(id);
        }
    }

    println!(
        "[API] Juegos públicos encontrados para {}: {} (universeIds)",
        user_id,
        universe_ids.len()
    );

    // 2) Para cada juego, obtener sus gamepasses
    for universe_id in universe_ids {
        let gp_url = format!(
            "https://games.roblox.com/v2/games/{}/game-passes?limit=100&sortOrder=Asc",
            universe_id
        );
        println!(
            "[API] Pidiendo game-passes del juego (universeId={}) en {}",
            universe_id, gp_url
        );

        let gp_resp = match reqwest::get(&gp_url).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "[API] Error HTTP al pedir game-passes de universeId {}: {}",
                    universe_id, e
                );
                continue;
            }
        };

        if !gp_resp.status().is_success() {
            eprintln!(
                "[API] game-passes HTTP {} para universeId={}",
                gp_resp.status(),
                universe_id
            );
            continue;
        }

        let gp_json: serde_json::Value = match gp_resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[API] Error parseando JSON de game-passes (universeId {}): {}",
                    universe_id, e
                );
                continue;
            }
        };

        let Some(passes_arr) = gp_json.get("data").and_then(|v| v.as_array()) else {
            println!(
                "[API] Sin 'data' en game-passes para universeId={}",
                universe_id
            );
            continue;
        };

        for pass in passes_arr {
            let Some(id) = pass.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let name = pass
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("GamePass")
                .to_string();

            // Evitar duplicados
            if !seen_ids.insert(id) {
                continue;
            }

            // 3) Obtener precio desde economy.roblox.com
            let detail_url = format!(
                "https://economy.roblox.com/v2/assets/{}/details",
                id
            );

            if let Ok(detail_resp) = reqwest::get(&detail_url).await {
                if let Ok(details) = detail_resp.json::<serde_json::Value>().await {
                    let price_i64 = details["PriceInRobux"]
                        .as_i64()
                        .or_else(|| details["Price"].as_i64())
                        .unwrap_or(0);

                    if price_i64 <= 0 {
                        continue;
                    }

                    let price = price_i64 as i32;
                    println!(
                        "[API] GamePass desde juegos públicos → id={}, name='{}', price={}",
                        id, name, price
                    );

                    result.push(Gamepass { id, name, price });
                }
            }
        }
    }

    println!(
        "[API] Total gamepasses (por juegos públicos) con precio > 0 para {}: {}",
        user_id,
        result.len()
    );

    result
}

/// Fallback: usa el catálogo global como antes, filtrando assetType=46 (GamePass)
async fn fetch_passes_from_catalog(user_id: u64) -> Vec<Gamepass> {
    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    let url = format!(
        "https://catalog.roblox.com/v1/search/items/details?creatorTargetId={}&creatorType=User&itemType=Asset&includeNotForSale=true&limit=30&sortType=Updated",
        user_id
    );
    println!(
        "[API] Pidiendo catálogo (fallback) para userId={} en {}",
        user_id, url
    );

    let resp = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[API] Error HTTP en catálogo: {e}");
            return result;
        }
    };

    if !resp.status().is_success() {
        eprintln!(
            "[API] Catálogo HTTP {} para userId={}",
            resp.status(),
            user_id
        );
        return result;
    }

    let data: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[API] Error parseando JSON de catálogo: {e}");
            return result;
        }
    };

    let Some(items) = data.get("data").and_then(|v| v.as_array()) else {
        println!("[API] Catálogo fallback: sin 'data' para userId={}", user_id);
        return result;
    };

    println!(
        "[API] Items de catálogo recibidos para {}: {}",
        user_id,
        items.len()
    );

    for item in items {
        // Filtrar SOLO assetType=46 (GamePass)
        let asset_type_id = item
            .get("assetType")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if asset_type_id != 46 {
            continue;
        }

        let Some(id) = item.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };

        if !seen_ids.insert(id) {
            continue;
        }

        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("GamePass")
            .to_string();

        let price = item
            .get("price")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if price <= 0 {
            continue;
        }

        println!(
            "[API] GamePass desde catálogo → id={}, name='{}', price={}",
            id, name, price
        );

        result.push(Gamepass {
            id,
            name,
            price: price as i32,
        });
    }

    println!(
        "[API] Total gamepasses (catálogo fallback) con precio > 0 para {}: {}",
        user_id,
        result.len()
    );

    result
}

// ---------- Handler principal ----------

#[tokio::main]
async fn main() {
    let app = Router::new().route("/user/:id/passes", get(get_passes));

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

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    println!("=====================================");
    println!("[API] /user/{}/passes", user_id);

    // 1) Primero intentamos por **juegos públicos**
    let mut passes = fetch_passes_from_public_games(user_id).await;

    // 2) Si no encontramos nada, usamos el catálogo como respaldo
    if passes.is_empty() {
        println!("[API] Sin gamepasses por juegos públicos, usando catálogo fallback…");
        passes = fetch_passes_from_catalog(user_id).await;
    }

    Json(ApiResponse {
        ok: true,
        user_id,
        count: passes.len(),
        passes,
    })
}



