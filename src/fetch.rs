use http::StatusCode;
use serde::de::DeserializeOwned;
use sigh::{PrivateKey, SigningConfig, alg::RsaSha256};
use crate::{digest, error::Error};

pub async fn authorized_fetch<T>(
    client: &reqwest::Client,
    uri: &str,
    key_id: &str,
    private_key: &PrivateKey,
) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let url = reqwest::Url::parse(uri)
        .map_err(|_| Error::InvalidUri)?;
    let host = format!("{}", url.host().ok_or(Error::InvalidUri)?);
    let digest_header = digest::generate_header(&[])
        .expect("digest::generate_header");
    let mut req = http::Request::builder()
        .uri(uri)
        .header("host", &host)
        .header("content-type", "application/activity+json")
        .header("date", chrono::Utc::now().to_rfc2822()
            .replace("+0000", "GMT"))
        .header("accept", "application/activity+json")
        .header("digest", digest_header)
        .body(vec![])?;
    SigningConfig::new(RsaSha256, private_key, key_id)
        .sign(&mut req)?;
    let req: reqwest::Request = req.try_into()?;
    let res = client.execute(req)
        .await?;
    if res.status() >= StatusCode::OK && res.status() < StatusCode::MULTIPLE_CHOICES {
        Ok(res.json().await?)
    } else {
        Err(Error::Response(format!("{}", res.text().await?)))
    }
}
