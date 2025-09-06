use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_LENGTH;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

#[cfg(feature = "reqwest")]
pub fn download_with_progress(client: &Client, url: &str, dest: &mut impl Write) -> io::Result<()> {
    let mut resp = client.get(url).send().map_err(io::Error::other)?;
    let total_size = resp
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().ok())
        .and_then(|ct_len| ct_len.parse().ok())
        .unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );


    let mut downloaded: u64 = 0;
    let mut buffer = [0; 8192];

    while let Ok(n) = resp.read(&mut buffer) {
        if n == 0 {
            break;
        }
        dest.write_all(&buffer[..n])?;
        downloaded += n as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");
    Ok(())
}
