use bluff_mau_mau::server::create_server;
use std::io::Write;

fn main() {
    if let Err(error) = run() {
        eprintln!("server: {error}");
        std::process::exit(2);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args().skip(1);
    let mut port = 8767;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .ok_or("--port requires a number")?
                    .parse::<u16>()
                    .map_err(|_| "port must be between 0 and 65535")?
            }
            "--help" | "-h" => {
                println!("Local human-versus-bot table and debug UI\nUsage: server [--port 8767]");
                return Ok(());
            }
            _ => return Err(format!("Unknown option: {arg}").into()),
        }
    }
    let server = create_server(port)?;
    println!("Bluff Mau-Mau: http://127.0.0.1:{}", server.port);
    std::io::stdout().flush()?;
    server.serve_forever()?;
    Ok(())
}
