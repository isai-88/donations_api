use std::{collections::HashSet, env, net::SocketAddr};

use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

// ----- Estructuras para (posible) respuesta de Open Cloud -----

#[derive(Deserialize)]
struct Experience {
    id: u64,
}

#[derive(Deserialize)]
struct ExperiencesResponse {
    data: Vec<Experience>,
}

// ----- Estructuras para gamepasses públicos de Roblox --------

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

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    // Cliente HTTP reutilizable
    let client = Client::new();

    // Leemos la API key de Open Cloud (si existe)
    let open_cloud_key = env::var("OPEN_CLOUD_API_KEY").ok();

    // 1) Intentar obtener los juegos del usuario con Open Cloud
    let mut place_ids: Vec<u64> = Vec::new();

    if let Some(api_key) = open_cloud_key {
        // ⚠️ IMPORTANTE:
        // Esta URL es un EJEMPLO. Tendrás que cambiarla al endpoint
        // real de Open Cloud que liste las experiencias/juegos del usuario.
        let url = format!(
            "https://apis.roblox.com/cloud/v2/users/{}/experiences",
            user_id
        );

        if let Ok(resp) = client
            .get(&url)
            .header("x-api-key", api_key)
            .send()
            .await
        {
            if let Ok(exps) = resp.json::<ExperiencesResponse>().await {
                for exp in exps.data {
                    place_ids.push(exp.id);
                }
            } else {
                println!("[OpenCloud] No se pudo parsear la lista de experiencias.");
            }
        } else {
            println!("[OpenCloud] Error llamando al endpoint de experiencias.");
        }
    }

    // 2) Si NO obtuvimos nada de Open Cloud, usar tu juego principal como fallback
    if place_ids.is_empty() {
        // Tu juego principal:
        place_ids.push(98889641203101u64);
    }

    // 3) Recorremos todos los juegos y buscamos sus gamepasses públicos
    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    for place_id in place_ids {
        let url = format!(
            "https://games.roblox.com/v2/games/{}/game-passes?limit=100&sortOrder=Asc",
            place_id
        );

        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(list) = resp.json::<RobloxPassList>().await {
                for pass in list.data {
                    let detail_url = format!(
                        "https://economy.roblox.com/v2/assets/{}/details",
                        pass.id
                    );

                    if let Ok(detail_resp) = client.get(&detail_url).send().await {
                        if let Ok(details) = detail_resp.json::<Value>().await {
                            let creator_id = details["Creator"]["Id"].as_u64().unwrap_or(0);

                            // Solo aceptar gamepasses cuyo creador sea ESTE usuario
                            if creator_id != user_id {
                                continue;
                            }

                            // Precio (puede venir en PriceInRobux o Price)
                            let price_i64 = details["PriceInRobux"]
                                .as_i64()
                                .or_else(|| details["Price"].as_i64())
                                .unwrap_or(0);

                            let price = price_i64 as i32;

                            // Ignorar gamepasses con precio 0 o negativo
                            if price <= 0 {
                                continue;
                            }

                            // Evitar duplicados si el mismo pass aparece en dos juegos
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

