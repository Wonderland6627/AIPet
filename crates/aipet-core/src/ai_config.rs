use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiApiConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider_cache: HashMap<String, ProviderSettings>,
}

impl Default for AiApiConfig {
    fn default() -> Self {
        Self {
            provider: "dashscope".into(),
            api_key: String::new(),
            base_url: "https://dashscope.aliyuncs.com/api/v1".into(),
            model: "wan2.7-image".into(),
            provider_cache: HashMap::new(),
        }
    }
}

impl AiApiConfig {
    pub fn normalize_cache(mut self) -> Self {
        if self.provider_cache.is_empty() {
            self.provider_cache.insert(
                self.provider.clone(),
                ProviderSettings {
                    api_key: self.api_key.clone(),
                    base_url: self.base_url.clone(),
                    model: self.model.clone(),
                },
            );
            return self;
        }

        if let Some(cached) = self.provider_cache.get(&self.provider) {
            if self.api_key.is_empty() {
                self.api_key = cached.api_key.clone();
            }
            if self.base_url.is_empty() {
                self.base_url = cached.base_url.clone();
            }
            if self.model.is_empty() {
                self.model = cached.model.clone();
            }
        }
        if !self.provider_cache.contains_key(&self.provider) {
            self.provider_cache.insert(
                self.provider.clone(),
                ProviderSettings {
                    api_key: self.api_key.clone(),
                    base_url: self.base_url.clone(),
                    model: self.model.clone(),
                },
            );
        }
        self
    }
}
