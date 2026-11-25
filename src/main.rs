use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use wcs_scada::{app, config::Config, web};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let path = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "config/plant.yaml".into()));
    let base = Config::load_base(&path)?;
    let cfg = Config::load(&path)?;
    tracing::info!(
        "impianto '{}': {} driver, {} aree, {} UDT ({} istanze), {} tag, {} punti di decisione",
        cfg.plant.name,
        cfg.drivers.len(),
        cfg.areas.len(),
        cfg.udt_types.len(),
        cfg.udt_instances.len(),
        cfg.tags.len(),
        cfg.decision_points.len()
    );
    let bind = cfg.server.bind.clone();
    let state = app::start(base, cfg).await?;
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("HMI: http://{bind}/   Designer: http://{bind}/designer.html");
    axum::serve(listener, web::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("arresto");
        })
        .await?;
    Ok(())
}
