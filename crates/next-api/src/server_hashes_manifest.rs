use anyhow::Result;
use serde::{Serialize, Serializer, ser::SerializeMap};
use turbo_tasks::{ResolvedVc, Vc};
use turbo_tasks_fs::{File, FileContent, FileSystemPath};
use turbopack_core::{
    asset::{Asset, AssetContent},
    output::{OutputAsset, OutputAssetsReference},
};

use crate::paths::{ServerPath, ServerPaths};

#[turbo_tasks::value]
pub struct ServerHashesManifestAsset {
    output_path: FileSystemPath,
    server_paths: ResolvedVc<ServerPaths>,
}

#[turbo_tasks::value_impl]
impl ServerHashesManifestAsset {
    #[turbo_tasks::function]
    pub fn new(output_path: FileSystemPath, server_paths: ResolvedVc<ServerPaths>) -> Vc<Self> {
        ServerHashesManifestAsset {
            output_path,
            server_paths,
        }
        .cell()
    }
}

#[turbo_tasks::value_impl]
impl OutputAssetsReference for ServerHashesManifestAsset {}

#[turbo_tasks::value_impl]
impl OutputAsset for ServerHashesManifestAsset {
    #[turbo_tasks::function]
    async fn path(&self) -> Vc<FileSystemPath> {
        self.output_path.clone().cell()
    }
}

#[turbo_tasks::value_impl]
impl Asset for ServerHashesManifestAsset {
    #[turbo_tasks::function]
    async fn content(&self) -> Result<Vc<AssetContent>> {
        let files = self.server_paths.await?;

        #[derive(Serialize)]
        struct Manifest<'a> {
            #[serde(serialize_with = "serialize_vec_as_map")]
            files: &'a Vec<ServerPath>,
        }

        let json = serde_json::to_string(&Manifest { files: &files })?;

        Ok(AssetContent::file(
            FileContent::Content(File::from(json)).cell(),
        ))
    }
}

fn serialize_vec_as_map<S>(list: &Vec<ServerPath>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(list.len()))?;
    for entry in list {
        map.serialize_entry(&entry.path, &entry.content_hash)?;
    }
    map.end()
}
