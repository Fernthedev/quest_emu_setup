use indicatif::{ProgressBar, ProgressStyle};

use std::io::{self, Read, Write};

#[cfg(feature = "reqwest")]
pub fn download_with_progress(
    client: Option<&reqwest::blocking::Client>,
    url: &str,
    dest: &mut impl Write,
) -> io::Result<()> {
    use std::sync::LazyLock;

    use reqwest::header::CONTENT_LENGTH;

    static DEFAULT_CLIENT: LazyLock<reqwest::blocking::Client> =
        LazyLock::new(reqwest::blocking::Client::new);

    let mut resp = client
        .unwrap_or(&DEFAULT_CLIENT)
        .get(url)
        .send()
        .map_err(io::Error::other)?;

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

#[cfg(feature = "ureq")]
pub fn download_with_progress(
    client: Option<&ureq::Agent>,
    url: &str,
    dest: &mut impl Write,
) -> io::Result<()> {
    use ureq::{http::header::CONTENT_LENGTH, Agent};

    use std::sync::LazyLock;
    static AGENT: LazyLock<Agent> = LazyLock::new(ureq::Agent::new_with_defaults);

    let mut resp = client
        .unwrap_or(&AGENT)
        .get(url)
        .call()
        .map_err(io::Error::other)?;

    let total_size = resp
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().unwrap().parse().ok())
        .unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    let mut downloaded: u64 = 0;
    let mut reader = resp.body_mut().as_reader();
    let mut buffer = [0; 8192];
    while let Ok(n) = reader.read(&mut buffer) {
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
