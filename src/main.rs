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

//
// Respuesta del endpoint:
// https://apis.roblox.com/game-passes/v1/users/{userId}/game-passes?count=100
//
#[derive(Deserialize)]
struct UserGamePassesResponse {
    data: Vec<UserGamePassSummary>,
    // cursors que no usamos, pero los dejamos por si acaso
    #[allow(dead_code)]
    previousPageCursor: Option<String>,
    #[allow(dead_code)]
    nextPageCursor: Option<String>,
}

#[derive(Deserialize)]
struct UserGamePassSummary {
    id: u64,
    name: String,
}

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    // 1) Pedimos TODOS los gamepasses del usuario
    let url = format!(
        "https://apis.roblox.com/game-passes/v1/users/{}/game-passes?count=100",
        user_id
    );

    if let Ok(resp) = reqwest::get(&url).await {
        if let Ok(list) = resp.json::<UserGamePassesResponse>().await {
            for pass in list.data {
                // 2) Para cada gamepass, pedimos detalles (precio, creador, etc.)
                let detail_url = format!(
                    "https://economy.roblox.com/v2/assets/{}/details",
                    pass.id
                );

                if let Ok(detail_resp) = reqwest::get(&detail_url).await {
                    if let Ok(details) = detail_resp.json::<serde_json::Value>().await {
                        let creator_id = details["Creator"]["Id"].as_u64().unwrap_or(0);

                        // Por seguridad, comprobamos que realmente sea del usuario
                        if creator_id == user_id {
                            let price_i64 = details["PriceInRobux"]
                                .as_i64()
                                .or_else(|| details["Price"].as_i64())
                                .unwrap_or(0);

                            let price = price_i64 as i32;

                            // Saltar gamepasses de 0 Robux
                            if price <= 0 {
                                continue;
                            }

                            // Evitar duplicados
                            if seen_ids.insert(pass.id) {
                                result.push(Gamepass {
                                    id: pass.id,
                                    name: pass.name.clone(),
                                    price,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Json(ApiResponse {
        ok: true,
        user_id,
        count: result.len(),
        passes: result,
    })
}

#[tokio::main]
async fn main() {
    // Ruta: /user/:id/passes
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
