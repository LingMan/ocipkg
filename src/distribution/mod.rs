//! Rust binding to [OCI distribution spec](https://github.com/opencontainers/distribution-spec)

mod client;
mod name;
mod reference;

pub use client::Client;
pub use name::Name;
pub use reference::Reference;

use crate::{error::*, Digest, ImageName};
use oci_spec::image::*;
use std::{fs, io::Read, path::Path};

/// Push image to registry
pub async fn push_image(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(Error::NotAFile(path.to_owned()));
    }
    let mut f = fs::File::open(&path)?;
    let mut ar = crate::image::Archive::new(&mut f);
    for (image_name, manifest) in ar.get_manifests()? {
        let client = Client::new(&image_name.registry_url()?, image_name.name.as_str())?;
        for layer in manifest.layers() {
            let digest = Digest::new(layer.digest())?;
            let mut entry = ar.get_blob(&digest)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            client.push_blob(&buf).await?;
        }
        let digest = Digest::new(manifest.config().digest())?;
        let mut entry = ar.get_blob(&digest)?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        client.push_blob(&buf).await?;
        client
            .push_manifest(image_name.reference.as_str(), &manifest)
            .await?;
    }
    Ok(())
}

/// Get image from registry and save it into local storage
pub async fn get_image(image_name: &ImageName) -> Result<()> {
    let ImageName {
        name, reference, ..
    } = image_name;
    let client = Client::new(&image_name.registry_url()?, name)?;
    let manifest = client.get_manifest(reference).await?;
    let dest = crate::config::image_dir(image_name)?;
    for layer in manifest.layers() {
        let blob = client.get_blob(layer.digest()).await?;
        match layer.media_type() {
            MediaType::ImageLayerGzip => {}
            MediaType::Other(ty) => {
                // application/vnd.docker.image.rootfs.diff.tar.gzip case
                if !ty.ends_with("tar.gzip") {
                    continue;
                }
            }
            _ => continue,
        }
        let buf = flate2::read::GzDecoder::new(blob.as_ref());
        tar::Archive::new(buf).unpack(dest)?;
        return Ok(());
    }
    Err(Error::MissingLayer)
}
