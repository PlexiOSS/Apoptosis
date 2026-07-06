use std::io::Cursor;

use chrono::{DateTime, Utc};
use image::{DynamicImage, ImageFormat};

use crate::{config::CONFIG, entity::Entity, types::asset::AssetMetadata};

const MAX_ASSET_SIZE: usize = 50 * 1024 * 1024; // 50 MB

#[derive(Clone, Copy)]
pub enum AssetContentType {
    Png,
    Jpg,
    Gif,
    WebP,
}

impl AssetContentType {
    pub fn image_format(&self) -> ImageFormat {
        match self {
            AssetContentType::Png => ImageFormat::Png,
            AssetContentType::Jpg => ImageFormat::Jpeg,
            AssetContentType::Gif => ImageFormat::Gif,
            AssetContentType::WebP => ImageFormat::WebP,
        }
    }
}

#[derive(Clone, Copy)]
pub enum AssetType {
    Banner,
    Avatar,
}

impl AssetType {
    /// Returns the folder name for the asset type.
    pub fn folder(&self) -> &'static str {
        match self {
            AssetType::Banner => "banners",
            AssetType::Avatar => "avatars",
        }
    }

    /// Returns the string representation of the asset type.
    pub fn as_str(&self) -> &'static str {
        match self {
            AssetType::Banner => "banner",
            AssetType::Avatar => "avatar",
        }
    }

    /// Returns the default asset to use
    pub fn default_asset(&self) -> &'static str {
        match self {
            AssetType::Banner => "banners/default.webp",
            AssetType::Avatar => "avatars/default.webp",
        }
    }
}

pub struct Asset {
    /// Asset type (banner, icon etc.)
    pub kind: AssetType,
    /// Asset file type (png, jpg etc.). This is only required for certain asset types and may be ignored by others
    pub content_type: AssetContentType,
    /// Asset content (image data)
    pub content: Vec<u8>,
}

/// Decodes the raw bytes from the asset into a DynamicImage.
pub fn decode_asset(payload: &Asset) -> Result<DynamicImage, crate::Error> {
    if payload.content.len() > MAX_ASSET_SIZE {
        return Err(format!(
            "Asset size exceeds maximum limit of {} bytes",
            MAX_ASSET_SIZE
        )
        .into());
    }

    let reader = Cursor::new(&payload.content);

    let img = image::load(reader, payload.content_type.image_format())
        .map_err(|e| format!("Error decoding PNG: {}", e))?;

    Ok(img)
}

/// Both decodes and saves the asset to disk
pub fn save_asset<E: Entity>(
    e: &E,
    id: &str,
    payload: &Asset,
) -> Result<(), crate::Error> {
    let img = decode_asset(payload)?;

    let asset_path = asset_realpath(&asset_path(e, id, payload.kind));

    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(&asset_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Error creating directories for asset path {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    // Save image as webp
    let mut output_file = std::fs::File::create(&asset_path).map_err(|e| {
        format!(
            "Error creating asset file at path {}: {}",
            asset_path, e
        )
    })?;

    img.write_to(&mut output_file, ImageFormat::WebP)
        .map_err(|e| format!("Error saving asset as WebP: {}", e))?;

    Ok(())
}

/// Returns the asset path for the given asset type and ID.
fn asset_path<E: Entity>(e: &E, id: &str, asset_type: AssetType) -> String {
    return format!("{}/{}/{}.webp", asset_type.folder(), e.cdn_folder(), id);
}

/// Returns the real asset path for the given asset path.
fn asset_realpath(path: &str) -> String {
    return format!("{}/{}", CONFIG.cdn_path, path);
}

/// Returns asset metadata for the given asset type and ID.
pub fn asset_metadata<E: Entity>(
    e: &E,
    id: &str,
    asset_type: AssetType,
) -> AssetMetadata {
    let path = asset_path(e, id, asset_type);
    let default_path = asset_type.default_asset().to_string();
    let kind = asset_type.as_str().to_string();

    let stat = match std::fs::metadata(asset_realpath(&path)) {
        Ok(s) => s,
        Err(e) => return AssetMetadata {
            exists: false,
            path,
            default_path,
            kind,
            last_modified: None,
            errors: vec![format!("Error getting asset metadata: {}", e)],
            size: 0,
        },
    };
    
    if stat.is_dir() {
        return AssetMetadata {
            exists: false,
            path,
            default_path,
            kind,
            last_modified: None,
            errors: vec!["Asset path is a directory".to_string()],
            size: 0,
        };
    }

    let mod_time = stat.modified().ok().and_then(|time| {
        DateTime::<Utc>::from(time).into()
    });

    AssetMetadata {
        exists: true,
        path,
        default_path,
        kind,
        size: stat.len(),
        last_modified: mod_time,
        errors: vec![],
    }
}

/// Resolves the asset metadata to a URL
pub fn resolve_asset_metadata_to_url(am: &AssetMetadata) -> String {
    if am.exists {
        let ts = match &am.last_modified {
            Some(dt) => dt.timestamp().to_string(),
            None => "0".to_string(),
        };
        format!("{}/{}?ts={}", CONFIG.cdn_url, am.path, ts)
    } else {
        format!("{}/{}", CONFIG.cdn_url, am.default_path)
    }
}

/// Deletes the asset of a given type and ID.
pub fn delete_asset<E: Entity>(
    e: &E,
    id: &str,
    asset_type: AssetType,
) -> Result<(), crate::Error> {
    let path = asset_realpath(&asset_path(e, id, asset_type));

    match std::fs::remove_file(&path) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(()) // If the file doesn't exist, consider it deleted
            } else {
                Err(format!("Error deleting asset file {}: {}", path, e).into())
            }
        }
    }
}