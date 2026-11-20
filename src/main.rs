//! nrzDashboard – Dashboard für umbrelOS. Dioxus fullstack + nrzUI.

mod api;
mod app;
mod components;
mod model;
mod pages;
mod utils;

#[cfg(feature = "server")]
mod server;

fn main() {
    #[cfg(feature = "server")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("healthcheck") {
            // Docker healthcheck: is the server accepting connections?
            let port = std::env::var("PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(8080);
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            let ok = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3)).is_ok();
            std::process::exit(i32::from(!ok));
        }
        if args.get(1).map(String::as_str) == Some("hash-password") {
            let Some(pw) = args.get(2) else {
                eprintln!("Aufruf: nrz_dashboard hash-password <passwort>");
                std::process::exit(2);
            };
            match server::auth::hash_password(pw) {
                Ok(h) => println!("{h}"),
                Err(e) => eprintln!("{e}"),
            }
            return;
        }
        dioxus::serve(|| async move { Ok(server::router(app::App)) });
    }

    #[cfg(not(feature = "server"))]
    dioxus::launch(app::App);
}
