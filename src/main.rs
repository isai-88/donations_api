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

#[derive(Deserialize)]
struct RobloxListResponse {
    data: Vec<RobloxPass>,
}

#[derive(Deserialize)]
struct RobloxPass {
    id: u64,
    name: String,
    #[allow(dead_code)]
    productId: u64,
}

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let games = vec![
        98889641203101u64,
    ];

    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    for place_id in games {
        let url = format!(
            "https://games.roblox.com/v2/games/{}/game-passes?limit=100&sortOrder=Asc",
            place_id
        );

        if let Ok(resp) = reqwest::get(&url).await {
            if let Ok(list) = resp.json::<RobloxListResponse>().await {
                for pass in list.data {
                    let detail_url = format!(
                        "https://economy.roblox.com/v2/assets/{}/details",
                        pass.id
                    );

                    if let Ok(detail_resp) = reqwest::get(&detail_url).await {
                        if let Ok(details) = detail_resp.json::<serde_json::Value>().await {
                            let creator_id = details["Creator"]["Id"].as_u64().unwrap_or(0);

                            if creator_id == user_id {
                                let price_i64 = details["PriceInRobux"]
                                    .as_i64()
                                    .or_else(|| details["Price"].as_i64())
                                    .unwrap_or(0);

                                let price = price_i64 as i32;

                                if price <= 0 {
                                    continue;
                                }

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
