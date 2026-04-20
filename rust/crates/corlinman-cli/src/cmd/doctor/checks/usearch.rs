//! usearch HNSW index check.
//!
//! Confirms that `data_dir/vector/index.usearch` opens and that its embedded
//! dimension matches what the active embedding model expects (best-effort —
//! we only enforce a dimension if the config resolves to a known model). A
//! mismatched dimension is a hard [`Fail`] because the agent cannot query
//! the index without it.

use std::path::PathBuf;

use async_trait::async_trait;
use corlinman_vector::UsearchIndex;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct UsearchCheck;

impl UsearchCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UsearchCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn usearch_path(ctx: &DoctorContext) -> PathBuf {
    let base = ctx
        .config
        .as_ref()
        .map(|c| c.server.data_dir.clone())
        .unwrap_or_else(|| ctx.data_dir.clone());
    base.join("vector").join("index.usearch")
}

/// Well-known (model_id, dimension) pairs. Used to spot-check the index
/// against the configured embedding model. Unknown models return None so
/// we skip the dim check instead of falsely flagging them.
fn expected_dim(model_id: &str) -> Option<usize> {
    match model_id {
        "mxbai-embed-large" => Some(1024),
        "nomic-embed-text" | "nomic-embed-text-v1" => Some(768),
        "bge-large-en" | "bge-large-en-v1.5" => Some(1024),
        "text-embedding-3-small" => Some(1536),
        "text-embedding-3-large" => Some(3072),
        _ => None,
    }
}

#[async_trait]
impl DoctorCheck for UsearchCheck {
    fn name(&self) -> &str {
        "usearch"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let path = usearch_path(ctx);
        if !path.exists() {
            return DoctorResult::Warn {
                message: format!("no usearch index at {}", path.display()),
                hint: Some("run `corlinman vector rebuild` to build one".into()),
            };
        }

        // If the configured embedding model is well known, open with dim check;
        // otherwise fall back to the dimension-agnostic `open`.
        let exp_dim = ctx
            .config
            .as_ref()
            .and_then(|c| expected_dim(&c.rag.embedding_model));

        match exp_dim {
            Some(d) => match UsearchIndex::open_checked(&path, d) {
                Ok(idx) => DoctorResult::Ok {
                    message: format!(
                        "{} opens; dim={}, size={}",
                        path.display(),
                        idx.dim(),
                        idx.size()
                    ),
                },
                Err(e) => DoctorResult::Fail {
                    message: format!("usearch open_checked(dim={d}) failed: {e}"),
                    hint: Some(
                        "run `corlinman vector rebuild` after changing embedding model".into(),
                    ),
                },
            },
            None => match UsearchIndex::open(&path) {
                Ok(idx) => DoctorResult::Ok {
                    message: format!(
                        "{} opens; dim={} (model dim unknown, skipped strict check)",
                        path.display(),
                        idx.dim()
                    ),
                },
                Err(e) => DoctorResult::Fail {
                    message: format!("usearch open failed: {e}"),
                    hint: Some("index file may be corrupt; rebuild it".into()),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx_for(data_dir: PathBuf) -> DoctorContext {
        DoctorContext {
            config_path: data_dir.join("config.toml"),
            data_dir,
            config: None,
        }
    }

    #[tokio::test]
    async fn missing_index_is_warn() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = UsearchCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {:?}", res);
    }

    #[tokio::test]
    async fn valid_index_is_ok() {
        let dir = tempdir().unwrap();
        let vdir = dir.path().join("vector");
        std::fs::create_dir_all(&vdir).unwrap();
        let idx_path = vdir.join("index.usearch");
        let mut idx = UsearchIndex::create(8).expect("create");
        idx.add(1, &[0.1_f32; 8]).expect("add");
        idx.save(&idx_path).expect("save");
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = UsearchCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }
}
