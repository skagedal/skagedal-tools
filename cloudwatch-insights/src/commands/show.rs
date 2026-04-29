use anyhow::Result;
use tokio::io::AsyncWriteExt;

use crate::cli::ShowArgs;
use crate::commands::fail;
use crate::paths;

pub async fn run(args: ShowArgs) -> Result<()> {
    let path = paths::latest_run_path();
    if !path.exists() {
        return Err(fail(
            2,
            format!(
                "no runs found ({} does not exist). Run `cloudwatch-insights query` first.",
                path.display()
            ),
        ));
    }
    if args.path {
        println!("{}", path.display());
        return Ok(());
    }
    let bytes = tokio::fs::read(&path).await?;
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}
