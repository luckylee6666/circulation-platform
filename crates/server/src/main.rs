use std::process::ExitCode;

use circulation_server::{bootstrap, build_app, config::Config};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("启动失败: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    config.ensure_dirs()?;

    let _guard = circulation_server::config::init_tracing(&config)?;

    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    let local_addr = listener.local_addr()?;
    tracing::info!("流转平台服务已启动: http://{local_addr}");
    tracing::info!("数据目录: {}", config.data_dir.display());

    let state = bootstrap(config)?;
    let app = build_app(state)?;

    // 带上连接信息，审计日志才能记录到客户端 IP
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
