use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use hf_hub::{api::sync::ApiBuilder, Repo, RepoType};
use tokenizers::Tokenizer;

pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Embedder {
    pub fn load() -> Result<Self> {
        let device = Device::Cpu;

        // Empty-string env vars corrupt cache paths and confuse hf-hub.
        // safety: single-threaded at this point; no other thread is reading env vars
        unsafe {
            for var in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "http_proxy",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy",
                "HF_ENDPOINT",
                "HF_HOME",
                "HF_HUB_URL",
            ] {
                if std::env::var(var).unwrap_or_default().is_empty() {
                    std::env::remove_var(var);
                }
            }
        }

        let token = std::env::var("HF_TOKEN").ok();
        let api = ApiBuilder::new().with_token(token).build()?;
        let repo = api.repo(Repo::new(
            "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            RepoType::Model,
        ));

        let tokenizer_path = repo.get("tokenizer.json")?;
        let weights_path = repo.get("model.safetensors")?;
        let config_path = repo.get("config.json")?;

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!(e))
            .context("failed to load tokenizer")?;

        let config_str = std::fs::read_to_string(config_path)?;
        let config: Config = serde_json::from_str(&config_str)?;

        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)? };

        let model = BertModel::load(vb, &config)?;

        Ok(Embedder {
            model,
            tokenizer,
            device,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!(e))?;

        let token_ids: Vec<u32> = encoding.get_ids().to_vec();
        let attention_mask: Vec<u32> = encoding.get_attention_mask().to_vec();
        let seq_len = token_ids.len();

        let token_ids = Tensor::new(token_ids.as_slice(), &self.device)?.unsqueeze(0)?;
        let attention_mask = Tensor::new(attention_mask.as_slice(), &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::zeros(token_ids.shape(), DType::U32, &self.device)?;

        // forward pass → [1, seq_len, 384]
        let hidden = self
            .model
            .forward(&token_ids, &token_type_ids, Some(&attention_mask))?;

        // mean-pool across seq_len → [1, 384]
        let embedding = (hidden.sum(1)? / seq_len as f64)?;

        let vec = embedding.squeeze(0)?.to_vec1::<f32>()?;

        // L2-normalize so all embeddings are unit vectors — cosine similarity
        // becomes pure angle comparison with no magnitude bias from chunk length... still not
        // totally working
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        Ok(vec.iter().map(|x| x / norm).collect())
    }
}
