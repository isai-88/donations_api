use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr};

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

// Respuesta de https://catalog.roblox.com/v1/search/items/details
#[derive(Deserialize)]
struct CatalogResponse {
    data: Vec<CatalogItem>,
}

#[derive(Deserialize)]
struct CatalogItem {
    id: u64,
    name: String,
    #[serde(default)]
    price: Option<i32>,
    // Si luego quieres filtrar por tipo (ropa, pass, etc.), aquí se puede añadir:
    // #[serde(default)]
    // assetType: Option<i32>,
}

// GET /user/:id/passes
async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let client = Client::new();

    // URL directa de Roblox (ya no usamos roproxy)
    let base_url = "https://catalog.roblox.com/v1/search/items/details";

    // Construimos la URL con los mismos parámetros que tu index.js
    let req = client
        .get(base_url)
        .query(&[
            ("creatorTargetId", user_id.to_string()),
            ("creatorType", "User".to_string()),
            ("itemType", "Asset".to_string()),
            // Puedes activar esto más adelante para solo ciertos tipos:
            // ("assetTypes", "Pass".to_string()),
            ("includeNotForSale", "true".to_string()),
            ("sortType", "Updated".to_string()),
            ("limit", "28".to_string()), // 10, 28 o 30 son válidos
        ]);

    println!(
        "[API] Pidiendo catálogo para userId={} en {}",
        user_id, base_url
    );

    let resp = req.send().await;

    let mut passes: Vec<Gamepass> = Vec::new();
    let mut ok_flag = true;

    match resp {
        Ok(r) => {
            if !r.status().is_success() {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                println!("[API] HTTP {} body: {}", status, text);
                ok_flag = false;
            } else {
                match r.json::<CatalogResponse>().await {
                    Ok(body) => {
                        println!(
                            "[API] Items recibidos para {}: {}",
                            user_id,
                            body.data.len()
                        );

                        for item in body.data {
                            if let Some(price) = item.price {
                                if price > 0 {
                                    passes.push(Gamepass {
                                        id: item.id,
                                        name: item.name.clone(),
                                        price,
                                    });
                                }
                            }
                        }

                        println!(
                            "[API] Total assets con precio > 0 para {}: {}",
                            user_id,
                            passes.len()
                        );
                    }
                    Err(err) => {
                        println!("[API] Error parseando JSON: {:?}", err);
                        ok_flag = false;
                    }
                }
            }
        }
        Err(err) => {
            println!("[API] Error haciendo request a catalog.roblox.com: {:?}", err);
            ok_flag = false;
        }
    }

    Json(ApiResponse {
        ok: ok_flag,
        user_id,
        count: passes.len(),
        passes,
    })
}

#[tokio::main]
async fn main() {
    // Ruta principal: igual que en tu index.js
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

