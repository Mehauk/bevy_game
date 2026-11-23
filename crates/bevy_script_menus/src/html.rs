use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};

#[derive(Asset, TypePath, Debug)]
pub struct HtmlAsset {
    pub source: String,
}

#[derive(Default, TypePath)]
pub struct HtmlLoader;

impl AssetLoader for HtmlLoader {
    type Asset = HtmlAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(HtmlAsset { source })
    }

    fn extensions(&self) -> &[&str] {
        &["html"]
    }
}
