mod obs;

use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    Ok(())
}
