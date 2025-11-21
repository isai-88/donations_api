use axum::{
    extract::Path,
    routing::get,
    Json, Router,
};
use serde::{Serialize, Deserialize};
use std::{collections::HashSet, env, net::SocketAddr};
use tokio::net::TcpListener;

// ---------- Estructuras para la respuesta de nuestra API ----------

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

// ---------- Estructuras para las respuestas de Roblox ----------

// Respuesta de: https://games.roblox.com/v2/users/{userId}/games...
#[derive(Deserialize)]
struct RobloxGamesResponse {
    data: Vec<RobloxGame>,
    #[serde(rename = "nextPageCursor")]
    next_page_cursor: Option<String>,
}

#[derive(Deserialize)]
struct RobloxGame {
    id: u64,          // universeId del juego
    #[serde(default)]
    name: String,
}

// Respuesta de: https://games.roblox.com/v1/games/{universeId}/game-passes...
#[derive(Deserialize)]
struct RobloxPassesResponse {
    data: Vec<RobloxPass>,
}

#[derive(Deserialize)]
struct RobloxPass {
    id: u64,
    name: String,
    #[serde(default)]
    price: Option<i32>,     // algunos endpoints la traen aquí
    #[serde(default)]
    productId: Option<u64>, // no lo usamos, pero lo dejamos por compatibilidad
}

// ---------- Funciones de ayuda ----------

/// Pide a Roblox los juegos públicos del usuario y devuelve sus universeIds
async fn fetch_user_universe_ids(user_id: u64) -> Result<Vec<u64>, reqwest::Error> {
    let mut universes = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut url = format!(
            "https://games.roblox.com/v2/users/{}/games?accessFilter=Public&sortOrder=Asc&limit=50",
            user_id
        );

        if let Some(ref c) = cursor {
            url.push_str("&cursor=");
            url.push_str(c);
        }

        let resp = reqwest::get(&url).await?;
        let data: RobloxGamesResponse = resp.json().await?;

        for game in data.data {
            universes.push(game.id);
        }

        if let Some(next) = data.next_page_cursor {
            cursor = Some(next);
        } else {
            break;
        }
    }

    Ok(universes)
}

/// Pide a Roblox los gamepasses de un universeId
async fn fetch_gamepasses_for_universe(universe_id: u64) -> Result<Vec<RobloxPass>, reqwest::Error> {
    let url = format!(
        "https://games.roblox.com/v1/games/{}/game-passes?limit=100&sortOrder=Asc",
        universe_id
    );

    let resp = reqwest::get(&url).await?;
    let data: RobloxPassesResponse = resp.json().await?;
    Ok(data.data)
}

// ---------- Handler principal: /user/:id/passes ----------

async fn get_passes(Path(user_id): Path<u64>) -> Json<ApiResponse> {
    // 1) Conseguimos los universeIds (juegos públicos) del usuario
    let universe_ids = match fetch_user_universe_ids(user_id).await {
        Ok(list) => list,
        Err(err) => {
            eprintln!("[ERROR] fetch_user_universe_ids: {err}");
            Vec::new()
        }
    };

    let mut result: Vec<Gamepass> = Vec::new();
    let mut seen_ids: HashSet<u64> = HashSet::new();

    // 2) Para cada juego, pedimos sus gamepasses
    for universe_id in universe_ids {
        if let Ok(passes) = fetch_gamepasses_for_universe(universe_id).await {
            for pass in passes {
                // Precio (si no viene, asumimos 0)
                let price = pass.price.unwrap_or(0);

                // ignoramos precios 0 o negativos
                if price <= 0 {
                    continue;
                }

                // evitamos duplicados por id de gamepass
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

    // Ordenar por precio (baratos primero)
    result.sort_by_key(|p| p.price);

    Json(ApiResponse {
        ok: true,
        user_id,
        count: result.len(),
        passes: result,
    })
}

// ---------- main: arranca el servidor en Railway ----------

#[tokio::main]
async fn main() {
    let app = Router::new().route("/user/:id/passes", get(get_passes));

    // Railway asigna PORT automáticamente, si no, usamos 8080
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 Rust API escuchando en {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
