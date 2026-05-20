use base64::Engine;
use image::imageops::FilterType;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::ai_config::AiApiConfig;

const DASHSCOPE_MIN_PIXELS: u64 = 589_824;
const DASHSCOPE_MAX_PIXELS: u64 = 16_777_216;
const OPENAI_MIN_PIXELS: u64 = 1_048_576; // 1024×1024

#[derive(Debug, Clone, Copy)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

pub struct AiApiClient {
    config: AiApiConfig,
    client: Client,
}

impl AiApiClient {
    pub fn new(config: AiApiConfig) -> Result<Self, String> {
        if config.api_key.is_empty() {
            return Err("API Key 未配置，请在设置中配置 AI API Key".into());
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| format!("create http client: {e}"))?;
        Ok(Self { config, client })
    }

    pub async fn text_to_image(&self, prompt: &str, size: ImageSize) -> Result<Vec<u8>, String> {
        match self.config.provider.as_str() {
            "dashscope" => self.dashscope_text_to_image(prompt, size).await,
            "gemini" => self.gemini_generate(prompt, None, size).await,
            _ => self.openai_text_to_image(prompt, size).await,
        }
    }

    pub async fn image_to_image(
        &self,
        image_data: &[u8],
        prompt: &str,
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        match self.config.provider.as_str() {
            "dashscope" => self.dashscope_image_edit(image_data, prompt, size).await,
            "gemini" => self.gemini_generate(prompt, Some(image_data), size).await,
            _ => self.openai_image_to_image(image_data, prompt, size).await,
        }
    }

    pub async fn text_and_image_to_image(
        &self,
        prompt: &str,
        image_data: &[u8],
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        self.image_to_image(image_data, prompt, size).await
    }

    /// Send text + multiple images to generate a new image.
    pub async fn text_and_images_to_image(
        &self,
        prompt: &str,
        images: &[&[u8]],
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        match self.config.provider.as_str() {
            "dashscope" => self.dashscope_multi_image(prompt, images, size).await,
            "gemini" => self.gemini_multi_image(prompt, images, size).await,
            _ => {
                // OpenAI only supports one image; use the first
                if let Some(first) = images.first() {
                    self.openai_image_to_image(first, prompt, size).await
                } else {
                    self.openai_text_to_image(prompt, size).await
                }
            }
        }
    }

    // ===== DashScope (阿里云百炼 wan2.7-image) =====

    async fn dashscope_text_to_image(&self, prompt: &str, size: ImageSize) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/services/aigc/multimodal-generation/generation",
            self.config.base_url
        );

        let (size_str, needs_resize) = self.dashscope_gen_size(size);
        let body = serde_json::json!({
            "model": self.config.model,
            "input": {
                "messages": [{
                    "role": "user",
                    "content": [{"text": prompt}]
                }]
            },
            "parameters": {
                "size": size_str,
                "n": 1,
                "watermark": false
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("DashScope 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("DashScope 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_dashscope_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, needs_resize)
    }

    async fn dashscope_image_edit(
        &self,
        image_data: &[u8],
        prompt: &str,
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/services/aigc/multimodal-generation/generation",
            self.config.base_url
        );

        let input_data = self.ensure_dashscope_min_input(image_data)?;
        let image_b64 = base64::engine::general_purpose::STANDARD.encode(&input_data);
        let image_data_url = format!("data:image/png;base64,{}", image_b64);
        let (size_str, needs_resize) = self.dashscope_gen_size(size);

        let body = serde_json::json!({
            "model": self.config.model,
            "input": {
                "messages": [{
                    "role": "user",
                    "content": [
                        {"image": image_data_url},
                        {"text": prompt}
                    ]
                }]
            },
            "parameters": {
                "size": size_str,
                "n": 1,
                "watermark": false
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("DashScope 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("DashScope 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_dashscope_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, needs_resize)
    }

    async fn dashscope_multi_image(
        &self,
        prompt: &str,
        images: &[&[u8]],
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/services/aigc/multimodal-generation/generation",
            self.config.base_url
        );

        let mut content = Vec::new();
        for img_data in images {
            let upscaled = self.ensure_dashscope_min_input(img_data)?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&upscaled);
            content.push(serde_json::json!({"image": format!("data:image/png;base64,{}", b64)}));
        }
        content.push(serde_json::json!({"text": prompt}));

        let (size_str, needs_resize) = self.dashscope_gen_size(size);
        let body = serde_json::json!({
            "model": self.config.model,
            "input": {
                "messages": [{"role": "user", "content": content}]
            },
            "parameters": {
                "size": size_str,
                "n": 1,
                "watermark": false
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("DashScope 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("DashScope 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_dashscope_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, needs_resize)
    }

    /// Compute generation size for DashScope.
    /// Caller guarantees ratio ≤ 4:1 (via batching).
    /// Only needs to handle min pixel upscale.
    fn dashscope_gen_size(&self, size: ImageSize) -> (String, bool) {
        let (mut gen_w, mut gen_h) = (size.width, size.height);

        let pixels = gen_w as u64 * gen_h as u64;
        if pixels < DASHSCOPE_MIN_PIXELS {
            let scale = (DASHSCOPE_MIN_PIXELS as f64 / pixels as f64).sqrt().ceil() as u32;
            gen_w *= scale;
            gen_h *= scale;
        } else if pixels > DASHSCOPE_MAX_PIXELS {
            let scale = (pixels as f64 / DASHSCOPE_MAX_PIXELS as f64).sqrt().ceil() as u32;
            gen_w /= scale;
            gen_h /= scale;
        }

        let needs_resize = gen_w != size.width || gen_h != size.height;
        (format!("{}*{}", gen_w, gen_h), needs_resize)
    }

    /// Uniform resize to target dimensions. Since batching ensures the generation
    /// aspect ratio matches the target, this is always a safe uniform scale.
    fn resize_image_if_needed(
        &self,
        image_bytes: Vec<u8>,
        target: ImageSize,
        needs_resize: bool,
    ) -> Result<Vec<u8>, String> {
        if !needs_resize {
            return Ok(image_bytes);
        }
        let img = image::load_from_memory(&image_bytes)
            .map_err(|e| format!("加载生成图片进行缩放失败: {e}"))?;
        let resized = img.resize_exact(target.width, target.height, FilterType::Lanczos3);
        let mut buf = std::io::Cursor::new(Vec::new());
        resized
            .write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("缩放后编码 PNG 失败: {e}"))?;
        Ok(buf.into_inner())
    }

    /// Ensure the input image meets DashScope's minimum 240x240 resolution requirement.
    fn ensure_dashscope_min_input(&self, image_data: &[u8]) -> Result<Vec<u8>, String> {
        const MIN_DIM: u32 = 240;
        let img = image::load_from_memory(image_data)
            .map_err(|e| format!("加载输入图片失败: {e}"))?;
        let (w, h) = (img.width(), img.height());
        if w >= MIN_DIM && h >= MIN_DIM {
            return Ok(image_data.to_vec());
        }
        let scale_w = if w < MIN_DIM { (MIN_DIM as f64 / w as f64).ceil() as u32 } else { 1 };
        let scale_h = if h < MIN_DIM { (MIN_DIM as f64 / h as f64).ceil() as u32 } else { 1 };
        let scale = scale_w.max(scale_h);
        let new_w = w * scale;
        let new_h = h * scale;
        let resized = img.resize_exact(new_w, new_h, FilterType::Lanczos3);
        let mut buf = std::io::Cursor::new(Vec::new());
        resized
            .write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("放大输入图片失败: {e}"))?;
        Ok(buf.into_inner())
    }

    fn extract_dashscope_image(&self, resp: &Value) -> Result<Vec<u8>, String> {
        // DashScope response: output.choices[0].message.content[N].image (URL or base64)
        let choices = resp
            .pointer("/output/choices")
            .and_then(|v| v.as_array())
            .ok_or("DashScope 响应格式异常: 无 choices")?;

        let content = choices
            .first()
            .and_then(|c| c.pointer("/message/content"))
            .and_then(|v| v.as_array())
            .ok_or("DashScope 响应格式异常: 无 content")?;

        for item in content {
            if let Some(image_val) = item.get("image").and_then(|v| v.as_str()) {
                if image_val.starts_with("data:") {
                    let b64_part = image_val.split(',').nth(1).unwrap_or("");
                    return base64::engine::general_purpose::STANDARD
                        .decode(b64_part)
                        .map_err(|e| format!("解码图片 base64 失败: {e}"));
                }
                // It's a URL - download it
                return self.download_image_sync(image_val);
            }
        }

        Err("DashScope 响应中未找到图片数据".into())
    }

    fn download_image_sync(&self, url: &str) -> Result<Vec<u8>, String> {
        let url = url.to_string();
        let client = self.client.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| format!("下载图片失败: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("下载图片返回: {}", resp.status()));
                }
                resp.bytes()
                    .await
                    .map(|b| b.to_vec())
                    .map_err(|e| format!("读取图片数据失败: {e}"))
            })
        })
    }

    // ===== OpenAI (gpt-image-1 / gpt-image-2) =====

    /// Compute generation size for OpenAI.
    /// Constraints:
    /// - max aspect ratio in [1:3, 3:1]
    /// - minimum pixel budget (provider-side dynamic, use conservative 1024×1024)
    fn openai_gen_size(&self, size: ImageSize) -> (String, bool) {
        let (mut gen_w, mut gen_h) = (size.width, size.height);
        let ratio = gen_w as f64 / gen_h as f64;
        if ratio > 3.0 {
            gen_h = (gen_w as f64 / 3.0).ceil() as u32;
        } else if ratio < (1.0 / 3.0) {
            gen_w = (gen_h as f64 / 3.0).ceil() as u32;
        }
        let pixels = gen_w as u64 * gen_h as u64;
        if pixels < OPENAI_MIN_PIXELS {
            let scale = (OPENAI_MIN_PIXELS as f64 / pixels as f64).sqrt().ceil() as u32;
            gen_w *= scale;
            gen_h *= scale;
        }
        let needs_resize = gen_w != size.width || gen_h != size.height;
        (format!("{}x{}", gen_w, gen_h), needs_resize)
    }

    async fn openai_text_to_image(&self, prompt: &str, size: ImageSize) -> Result<Vec<u8>, String> {
        let url = format!("{}/images/generations", self.config.base_url);
        let (gen_size, needs_resize) = self.openai_gen_size(size);

        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "n": 1,
            "size": gen_size,
            "quality": "medium"
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_openai_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, needs_resize)
    }

    async fn openai_image_to_image(
        &self,
        image_data: &[u8],
        prompt: &str,
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        let url = format!("{}/images/edits", self.config.base_url);
        let (gen_size, needs_resize) = self.openai_gen_size(size);

        let image_part = reqwest::multipart::Part::bytes(image_data.to_vec())
            .file_name("image.png")
            .mime_str("image/png")
            .map_err(|e| format!("create multipart: {e}"))?;

        let form = reqwest::multipart::Form::new()
            .text("model", self.config.model.clone())
            .part("image", image_part)
            .text("prompt", prompt.to_string())
            .text("n", "1")
            .text("size", gen_size)
            .text("quality", "medium");

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("OpenAI 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_openai_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, needs_resize)
    }

    fn extract_openai_image(&self, resp: &Value) -> Result<Vec<u8>, String> {
        let data = resp
            .get("data")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or("OpenAI 响应中无图片数据")?;

        if let Some(b64) = data.get("b64_json").and_then(|v| v.as_str()) {
            return base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| format!("解码 base64 图片失败: {e}"));
        }

        if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
            return self.download_image_sync(url);
        }

        Err("OpenAI 响应中无有效图片数据".into())
    }

    // ===== Gemini (Nano Banana) =====

    async fn gemini_generate(
        &self,
        prompt: &str,
        image_data: Option<&[u8]>,
        _size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/models/{}:generateContent",
            self.config.base_url, self.config.model
        );

        let mut parts: Vec<Value> = Vec::new();

        if let Some(img) = image_data {
            let b64 = base64::engine::general_purpose::STANDARD.encode(img);
            parts.push(serde_json::json!({
                "inlineData": {
                    "mimeType": "image/png",
                    "data": b64
                }
            }));
        }

        parts.push(serde_json::json!({"text": prompt}));

        let body = serde_json::json!({
            "contents": [{
                "parts": parts
            }],
            "generationConfig": {
                "responseModalities": ["IMAGE", "TEXT"]
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        self.extract_gemini_image(&result)
    }

    async fn gemini_multi_image(
        &self,
        prompt: &str,
        images: &[&[u8]],
        size: ImageSize,
    ) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/models/{}:generateContent",
            self.config.base_url, self.config.model
        );

        let mut parts: Vec<Value> = Vec::new();
        for img in images {
            let b64 = base64::engine::general_purpose::STANDARD.encode(img);
            parts.push(serde_json::json!({
                "inlineData": {
                    "mimeType": "image/png",
                    "data": b64
                }
            }));
        }
        parts.push(serde_json::json!({"text": prompt}));

        let body = serde_json::json!({
            "contents": [{"parts": parts}],
            "generationConfig": {
                "responseModalities": ["IMAGE", "TEXT"]
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini 请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini 返回错误 {status}: {text}"));
        }

        let result: Value = resp.json().await.map_err(|e| format!("解析响应失败: {e}"))?;
        let image_bytes = self.extract_gemini_image(&result)?;
        self.resize_image_if_needed(image_bytes, size, true)
    }

    fn extract_gemini_image(&self, resp: &Value) -> Result<Vec<u8>, String> {
        // Gemini response: candidates[0].content.parts[N].inlineData.data
        let parts = resp
            .pointer("/candidates/0/content/parts")
            .and_then(|v| v.as_array())
            .ok_or("Gemini 响应格式异常: 无 parts")?;

        for part in parts {
            if let Some(inline) = part.get("inlineData") {
                if let Some(data_b64) = inline.get("data").and_then(|v| v.as_str()) {
                    return base64::engine::general_purpose::STANDARD
                        .decode(data_b64)
                        .map_err(|e| format!("解码 Gemini 图片失败: {e}"));
                }
            }
        }

        Err("Gemini 响应中未找到图片数据".into())
    }
}
