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
    // Tipo de asset (ej: 46 = Pass/GamePass, otros números = ropa, accesorios, etc.)
    #[serde(default)]
    #[serde(rename = "assetType")]
    asset_type: Option<i32>,
}

// GET /user/:id/passes
async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    let client = Client::new();

    let base_url = "https://catalog.roblox.com/v1/search/items/details";

    // Mismos parámetros que tu index.js, pero ahora vamos a filtrar por assetType en el código.
    let req = client
        .get(base_url)
        .query(&[
            ("creatorTargetId", user_id.to_string()),
            ("creatorType", "User".to_string()),
            ("itemType", "Asset".to_string()),
            // Si más adelante quieres que solo busque Pass desde la URL:
            // ("assetTypes", "Pass".to_string()),
            ("includeNotForSale", "true".to_string()),
            ("sortType", "Updated".to_string()),
            ("limit", "28".to_string()),
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
                            // 1) Sacar precio (si no viene, lo ignoramos)
                            let price = match item.price {
                                Some(p) if p > 0 => p,
                                _ => continue, // sin precio o 0 → no lo usamos
                            };

                            // 2) Ver assetType: queremos solo GamePass
                            //    En tu JSON viejo, los GamePass salían con assetType = 46
                            let asset_type = item.asset_type.unwrap_or(-1);
                            if asset_type != 46 {
                                // No es GamePass → lo saltamos
                                continue;
                            }

                            passes.push(Gamepass {
                                id: item.id,
                                name: item.name.clone(),
                                price,
                            });
                        }

                        println!(
                            "[API] Total GAMEPASSES (assetType=46) con precio > 0 para {}: {}",
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


