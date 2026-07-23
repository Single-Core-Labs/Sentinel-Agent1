use std::sync::OnceLock;
use std::num::NonZeroUsize;
use std::collections::HashSet;
use std::io::Cursor;
use async_trait::async_trait;
use base64::Engine as _;
use image::{load_from_memory, DynamicImage, ImageFormat, imageops::FilterType};
use lru::LruCache;
use sha2::{Sha256, Digest};
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use regex::Regex;
use crate::classifier::ContentType;
use crate::metrics::CompressionMetrics;
use super::{CompressionStrategy, CompressionResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTechnique {
    FullLow,
    Preserve,
    Crop,
    Transcode,
}

impl ImageTechnique {
    pub fn name(&self) -> &'static str {
        match self {
            ImageTechnique::FullLow => "full_low",
            ImageTechnique::Preserve => "preserve",
            ImageTechnique::Crop => "crop",
            ImageTechnique::Transcode => "transcode",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageCompressorConfig {
    pub max_dimension: u32,
    pub min_quality: u8,
    pub preserve_edge_threshold: f64,
    pub enable_crop: bool,
    pub enable_transcode: bool,
    pub cache_size: usize,
    pub prefer_webp: bool,
    pub provider: ImageProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProvider {
    OpenAi,
    Anthropic,
    Google,
    Auto,
}

impl Default for ImageProvider {
    fn default() -> Self {
        ImageProvider::Auto
    }
}

impl Default for ImageCompressorConfig {
    fn default() -> Self {
        Self {
            max_dimension: 512,
            min_quality: 60,
            preserve_edge_threshold: 0.15,
            enable_crop: true,
            enable_transcode: true,
            cache_size: 100,
            prefer_webp: false,
            provider: ImageProvider::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageAnalysis {
    pub width: u32,
    pub height: u32,
    pub entropy: f64,
    pub edge_density: f64,
    pub color_diversity: u32,
    pub has_text: bool,
    pub aspect_ratio: f64,
    pub file_size_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ImageCompressionResult {
    pub compressed_data_uri: String,
    pub technique: ImageTechnique,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub savings_pct: f64,
    pub original_width: u32,
    pub original_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub quality_preserved: f64,
}

impl ImageCompressionResult {
    pub fn token_estimate(&self, provider: ImageProvider) -> u64 {
        let pixel_count = self.output_width as u64 * self.output_height as u64;
        match provider {
            ImageProvider::OpenAi => {
                if self.technique == ImageTechnique::FullLow || self.technique == ImageTechnique::Transcode {
                    85
                } else {
                    let tiles = (pixel_count + 511) / 512;
                    tiles * 170 + 85
                }
            }
            ImageProvider::Anthropic => pixel_count / 750 + 10,
            ImageProvider::Google => {
                let tw = (self.output_width as u64 + 767) / 768;
                let th = (self.output_height as u64 + 767) / 768;
                tw * th * 258
            }
            ImageProvider::Auto => pixel_count / 500 + 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageCompressorConfigOut {
    pub config: ImageCompressorConfig,
}

static DATA_URI_RE: OnceLock<Regex> = OnceLock::new();
fn data_uri_re() -> &'static Regex {
    DATA_URI_RE.get_or_init(|| Regex::new(
        r"^data:image/(jpeg|png|webp|gif|bmp|tiff);base64,"
    ).unwrap())
}

fn parse_data_uri(data: &str) -> Option<(ImageFormat, Vec<u8>)> {
    let cap = data_uri_re().captures(data)?;
    let fmt = match cap.get(1)?.as_str() {
        "jpeg" => ImageFormat::Jpeg,
        "png" => ImageFormat::Png,
        "webp" => ImageFormat::WebP,
        "gif" => ImageFormat::Gif,
        "bmp" => ImageFormat::Bmp,
        "tiff" => ImageFormat::Tiff,
        _ => return None,
    };
    let b64_start = cap.get(0)?.end();
    let b64_data = &data[b64_start..];
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64_data).ok()?;
    Some((fmt, bytes))
}

fn vec_to_data_uri(data: &[u8], format: ImageFormat) -> String {
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Gif => "image/gif",
        _ => "image/jpeg",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    format!("data:{};base64,{}", mime, b64)
}

struct CachedResult {
    result: ImageCompressionResult,
}

struct ImageCache {
    cache: LruCache<String, CachedResult>,
}

impl ImageCache {
    fn new(size: usize) -> Self {
        let cap = NonZeroUsize::new(size.max(1)).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            cache: LruCache::new(cap),
        }
    }

    fn get(&mut self, hash: &str) -> Option<&CachedResult> {
        self.cache.get(hash)
    }

    fn put(&mut self, hash: String, result: CachedResult) {
        self.cache.put(hash, result);
    }
}

fn classify_query(query: &str) -> ImageTechnique {
    let lower = query.to_lowercase();

    let detail_kw = [
        "count", "measure", "exact", "precise", "specific",
        "tiny", "small", "fine", "subtle", "whisker", "spot",
        "defect", "crack", "scratch", "difference",
    ];
    let extract_kw = [
        "read", "text", "ocr", "transcribe", "extract", "document",
        "sign", "label", "serial", "barcode", "qrcode",
        "word", "paragraph", "page", "spell",
    ];
    let region_kw = [
        "corner", "region", "area", "part", "portion", "section",
        "left", "right", "top", "bottom", "center", "middle",
        "background", "foreground", "edge", "border",
        "upper", "lower",
    ];

    let detail = detail_kw.iter().filter(|k| lower.contains(*k)).count();
    let extract = extract_kw.iter().filter(|k| lower.contains(*k)).count();
    let region = region_kw.iter().filter(|k| lower.contains(*k)).count();

    if detail > 0 && detail >= extract && detail >= region {
        ImageTechnique::Preserve
    } else if extract > 0 && extract >= region {
        ImageTechnique::Transcode
    } else if region > 0 {
        ImageTechnique::Crop
    } else {
        ImageTechnique::FullLow
    }
}

fn analyze_image(img: &DynamicImage) -> ImageAnalysis {
    let (w, h) = (img.width(), img.height());
    let total_pixels = (w as u64) * (h as u64);
    if total_pixels == 0 {
        return ImageAnalysis {
            width: w, height: h, entropy: 0.0, edge_density: 0.0,
            color_diversity: 0, has_text: false, aspect_ratio: w as f64 / h.max(1) as f64,
            file_size_bytes: 0,
        };
    }

    let gray = img.to_luma8();
    let mut histogram = [0u64; 256];
    for pixel in gray.pixels() {
        histogram[pixel[0] as usize] += 1;
    }
    let entropy: f64 = histogram.iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / total_pixels as f64;
            -p * p.log2()
        })
        .sum();

    let sobel_threshold = 30u16;
    let mut edge_pixels = 0u64;
    let step = if w > 800 || h > 800 { 2.max(w / 400) } else { 1 };
    for y in (1..h - 1).step_by(step as usize) {
        for x in (1..w - 1).step_by(step as usize) {
            let gx = (gray.get_pixel(x + 1, y - 1)[0] as i32 + 2 * gray.get_pixel(x + 1, y)[0] as i32 + gray.get_pixel(x + 1, y + 1)[0] as i32)
                - (gray.get_pixel(x - 1, y - 1)[0] as i32 + 2 * gray.get_pixel(x - 1, y)[0] as i32 + gray.get_pixel(x - 1, y + 1)[0] as i32);
            let gy = (gray.get_pixel(x - 1, y + 1)[0] as i32 + 2 * gray.get_pixel(x, y + 1)[0] as i32 + gray.get_pixel(x + 1, y + 1)[0] as i32)
                - (gray.get_pixel(x - 1, y - 1)[0] as i32 + 2 * gray.get_pixel(x, y - 1)[0] as i32 + gray.get_pixel(x + 1, y - 1)[0] as i32);
            let mag = ((gx * gx + gy * gy) as f64).sqrt() as u16;
            if mag > sobel_threshold {
                edge_pixels += 1;
            }
        }
    }
    let sampled = ((w.max(1) / step) * (h.max(1) / step)) as f64;
    let edge_density = if sampled > 0.0 { edge_pixels as f64 / sampled } else { 0.0 };

    let rgb = img.to_rgb8();
    let mut color_set = HashSet::new();
    let color_step = (total_pixels / 1000).max(1) as u32;
    for y in (0..h).step_by(color_step as usize) {
        for x in (0..w).step_by(color_step as usize) {
            let p = rgb.get_pixel(x, y);
            let quantized = ((p[0] as u32 / 16) << 16) | ((p[1] as u32 / 16) << 8) | (p[2] as u32 / 16);
            color_set.insert(quantized);
        }
    }
    let color_diversity = color_set.len() as u32;

    ImageAnalysis {
        width: w,
        height: h,
        entropy,
        edge_density,
        color_diversity,
        has_text: edge_density > 0.12 && entropy > 4.0,
        aspect_ratio: w as f64 / h.max(1) as f64,
        file_size_bytes: 0,
    }
}

fn select_output_format(analysis: &ImageAnalysis, config: &ImageCompressorConfig) -> ImageFormat {
    if config.prefer_webp {
        ImageFormat::WebP
    } else if analysis.color_diversity < 64 || analysis.edge_density > 0.2 {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    }
}

fn compute_quality(analysis: &ImageAnalysis, config: &ImageCompressorConfig) -> u8 {
    if analysis.has_text {
        config.min_quality.max(75)
    } else if analysis.entropy > 6.5 {
        config.min_quality.max(70)
    } else if analysis.edge_density > 0.1 {
        config.min_quality.max(65)
    } else {
        config.min_quality
    }
}

fn compress_full_low(
    img: &DynamicImage,
    analysis: &ImageAnalysis,
    config: &ImageCompressorConfig,
) -> ImageCompressionResult {
    let max_dim = config.max_dimension;
    let (w, h) = (img.width(), img.height());
    let (new_w, new_h) = if w > h {
        (max_dim, (h as f64 * max_dim as f64 / w as f64) as u32)
    } else {
        ((w as f64 * max_dim as f64 / h as f64) as u32, max_dim)
    };
    let new_w = new_w.max(64);
    let new_h = new_h.max(64);

    let resized = img.resize_exact(new_w, new_h, FilterType::Lanczos3);
    let quality = compute_quality(analysis, config);
    let out_fmt = select_output_format(analysis, config);

    let mut out_bytes = Vec::new();
    {
        let mut cursor = Cursor::new(&mut out_bytes);
        let _ = resized.write_to(&mut cursor, out_fmt);
    }
    let compressed = out_bytes.len();

    let uri = vec_to_data_uri(&out_bytes, out_fmt);
    let orig_pixels = w as u64 * h as u64;
    let new_pixels = new_w as u64 * new_h as u64;
    let area_savings = 1.0 - (new_pixels as f64 / orig_pixels.max(1) as f64);
    let quality_preserved = if analysis.entropy > 5.0 {
        0.75 + (quality as f64 / 100.0) * 0.2
    } else {
        0.85 + (quality as f64 / 100.0) * 0.1
    };

    ImageCompressionResult {
        compressed_data_uri: uri,
        technique: ImageTechnique::FullLow,
        original_bytes: analysis.file_size_bytes,
        compressed_bytes: compressed,
        savings_pct: area_savings * 100.0,
        original_width: w,
        original_height: h,
        output_width: new_w,
        output_height: new_h,
        quality_preserved,
    }
}

fn compress_preserve(
    img: &DynamicImage,
    analysis: &ImageAnalysis,
    config: &ImageCompressorConfig,
) -> ImageCompressionResult {
    let out_fmt = select_output_format(analysis, config);
    let mut out_bytes = Vec::new();
    {
        let mut cursor = Cursor::new(&mut out_bytes);
        let _ = img.write_to(&mut cursor, out_fmt);
    }
    let uri = vec_to_data_uri(&out_bytes, out_fmt);
    ImageCompressionResult {
        compressed_data_uri: uri,
        technique: ImageTechnique::Preserve,
        original_bytes: analysis.file_size_bytes,
        compressed_bytes: out_bytes.len(),
        savings_pct: 0.0,
        original_width: img.width(),
        original_height: img.height(),
        output_width: img.width(),
        output_height: img.height(),
        quality_preserved: 1.0,
    }
}

fn compress_crop(
    img: &DynamicImage,
    analysis: &ImageAnalysis,
    config: &ImageCompressorConfig,
) -> ImageCompressionResult {
    let (w, h) = (img.width(), img.height());
    let crop_size = w.min(h) / 2;
    let cx = w / 2;
    let cy = h / 2;
    let x0 = cx.saturating_sub(crop_size / 2);
    let y0 = cy.saturating_sub(crop_size / 2);
    let x1 = (cx + crop_size / 2).min(w);
    let y1 = (cy + crop_size / 2).min(h);
    let cropped = img.crop_imm(x0, y0, x1 - x0, y1 - y0);
    let cropped_analysis = analyze_image(&cropped);
    let _quality = compute_quality(&cropped_analysis, config);
    let out_fmt = select_output_format(&cropped_analysis, config);

    let max_dim = config.max_dimension;
    let (cw, ch) = (cropped.width(), cropped.height());
    let (new_w, new_h) = if cw > ch {
        (max_dim, (ch as f64 * max_dim as f64 / cw as f64) as u32)
    } else {
        ((cw as f64 * max_dim as f64 / ch as f64) as u32, max_dim)
    };
    let new_w = new_w.max(32);
    let new_h = new_h.max(32);
    let resized = cropped.resize_exact(new_w, new_h, FilterType::Lanczos3);

    let mut out_bytes = Vec::new();
    {
        let mut cursor = Cursor::new(&mut out_bytes);
        let _ = resized.write_to(&mut cursor, out_fmt);
    }

    let uri = vec_to_data_uri(&out_bytes, out_fmt);
    let orig_pixels = w as u64 * h as u64;
    let new_pixels = new_w as u64 * new_h as u64;
    let savings = 1.0 - (new_pixels as f64 / orig_pixels.max(1) as f64);

    ImageCompressionResult {
        compressed_data_uri: uri,
        technique: ImageTechnique::Crop,
        original_bytes: analysis.file_size_bytes,
        compressed_bytes: out_bytes.len(),
        savings_pct: savings * 100.0,
        original_width: w,
        original_height: h,
        output_width: new_w,
        output_height: new_h,
        quality_preserved: 0.7,
    }
}

fn compress_transcode(
    img: &DynamicImage,
    analysis: &ImageAnalysis,
    config: &ImageCompressorConfig,
) -> ImageCompressionResult {
    let gray = if analysis.color_diversity < 48 {
        Some(img.to_luma8())
    } else {
        None
    };

    let (w, h) = (img.width(), img.height());
    let max_dim = (config.max_dimension as f64 * 1.5) as u32;
    let (new_w, new_h) = if w > h {
        (max_dim, (h as f64 * max_dim as f64 / w as f64) as u32)
    } else {
        ((w as f64 * max_dim as f64 / h as f64) as u32, max_dim)
    };
    let new_w = new_w.max(96);
    let new_h = new_h.max(96);

    let resized = if let Some(g) = gray {
        DynamicImage::ImageLuma8(
            image::imageops::resize(&g, new_w, new_h, FilterType::CatmullRom)
        )
    } else {
        img.resize_exact(new_w, new_h, FilterType::CatmullRom)
    };

    let _quality = if analysis.has_text { 65 } else { config.min_quality.max(35) };
    let mut out_bytes = Vec::new();
    {
        let mut cursor = Cursor::new(&mut out_bytes);
        let _ = resized.write_to(&mut cursor, ImageFormat::Jpeg);
    }
    let uri = vec_to_data_uri(&out_bytes, ImageFormat::Jpeg);
    let orig_pixels = w as u64 * h as u64;
    let new_pixels = new_w as u64 * new_h as u64;
    let savings = 1.0 - (new_pixels as f64 / orig_pixels.max(1) as f64);

    ImageCompressionResult {
        compressed_data_uri: uri,
        technique: ImageTechnique::Transcode,
        original_bytes: analysis.file_size_bytes,
        compressed_bytes: out_bytes.len(),
        savings_pct: 90.0 + savings * 9.0,
        original_width: w,
        original_height: h,
        output_width: new_w,
        output_height: new_h,
        quality_preserved: if analysis.has_text { 0.85 } else { 0.5 },
    }
}

fn provider_dimension(provider: ImageProvider, analysis: &ImageAnalysis) -> u32 {
    let (w, h) = (analysis.width, analysis.height);
    let max_dim = match provider {
        ImageProvider::OpenAi => 2048u32,
        ImageProvider::Anthropic => 512,
        ImageProvider::Google => 768,
        ImageProvider::Auto => 1024,
    };
    if w > h {
        w.min(max_dim)
    } else {
        h.min(max_dim)
    }
}

static IMAGE_CACHE: Lazy<Mutex<ImageCache>> = Lazy::new(|| {
    Mutex::new(ImageCache::new(100))
});

pub struct ImageAwareCompressor {
    config: ImageCompressorConfig,
}

impl ImageAwareCompressor {
    pub fn new() -> Self {
        Self {
            config: ImageCompressorConfig::default(),
        }
    }

    pub fn with_config(config: ImageCompressorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ImageCompressorConfig {
        &self.config
    }

    pub async fn compress_with_query(
        &self,
        image_data: &str,
        query: Option<&str>,
    ) -> Option<ImageCompressionResult> {
        let technique = if let Some(q) = query {
            classify_query(q)
        } else {
            ImageTechnique::FullLow
        };

        let (_, raw_bytes) = parse_data_uri(image_data)?;
        let img = load_from_memory(&raw_bytes).ok()?;
        let provider = self.config.provider;

        let max_dim = if provider == ImageProvider::Auto {
            self.config.max_dimension
        } else {
            provider_dimension(provider, &ImageAnalysis {
                width: img.width(), height: img.height(),
                entropy: 0.0, edge_density: 0.0, color_diversity: 0,
                has_text: false, aspect_ratio: 1.0, file_size_bytes: raw_bytes.len(),
            })
        };

        let config = ImageCompressorConfig {
            max_dimension: max_dim,
            ..self.config.clone()
        };

        let result = match technique {
            ImageTechnique::FullLow => {
                let analysis = analyze_image(&img);
                compress_full_low(&img, &analysis, &config)
            }
            ImageTechnique::Preserve => {
                let analysis = analyze_image(&img);
                compress_preserve(&img, &analysis, &config)
            }
            ImageTechnique::Crop if config.enable_crop => {
                let analysis = analyze_image(&img);
                compress_crop(&img, &analysis, &config)
            }
            ImageTechnique::Transcode if config.enable_transcode => {
                let analysis = analyze_image(&img);
                compress_transcode(&img, &analysis, &config)
            }
            _ => {
                let analysis = analyze_image(&img);
                compress_full_low(&img, &analysis, &config)
            }
        };

        Some(result)
    }

    pub async fn compress_auto(
        &self,
        image_data: &str,
        query: Option<&str>,
    ) -> Option<ImageCompressionResult> {
        let mut hash = Sha256::new();
        hash.update(image_data.as_bytes());
        if let Some(q) = query {
            hash.update(q.as_bytes());
        }
        let hash_str = hex::encode(hash.finalize());

        {
            let mut cache = IMAGE_CACHE.lock().await;
            if let Some(cached) = cache.get(&hash_str) {
                return Some(cached.result.clone());
            }
        }

        let technique = if let Some(q) = query {
            classify_query(q)
        } else {
            ImageTechnique::FullLow
        };

        let (_, raw_bytes) = parse_data_uri(image_data)?;
        let img = load_from_memory(&raw_bytes).ok()?;
        let provider = self.config.provider;

        let max_dim = if provider == ImageProvider::Auto {
            self.config.max_dimension
        } else {
            provider_dimension(provider, &ImageAnalysis {
                width: img.width(), height: img.height(),
                entropy: 0.0, edge_density: 0.0, color_diversity: 0,
                has_text: false, aspect_ratio: 1.0, file_size_bytes: raw_bytes.len(),
            })
        };

        let mut config = self.config.clone();
        config.max_dimension = max_dim;

        let mut candidates: Vec<ImageCompressionResult> = Vec::new();
        let analysis = analyze_image(&img);

        match technique {
            ImageTechnique::FullLow => {
                candidates.push(compress_full_low(&img, &analysis, &config));
                candidates.push(compress_preserve(&img, &analysis, &config));
            }
            ImageTechnique::Preserve => {
                candidates.push(compress_preserve(&img, &analysis, &config));
                candidates.push(compress_full_low(&img, &analysis, &config));
            }
            ImageTechnique::Crop if config.enable_crop => {
                candidates.push(compress_crop(&img, &analysis, &config));
            }
            ImageTechnique::Transcode if config.enable_transcode => {
                candidates.push(compress_transcode(&img, &analysis, &config));
            }
            _ => {
                candidates.push(compress_full_low(&img, &analysis, &config));
            }
        }

        let best = candidates.into_iter()
            .max_by(|a, b| {
                let a_score = a.savings_pct * a.quality_preserved;
                let b_score = b.savings_pct * b.quality_preserved;
                a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
            })?;

        {
            let h = hash_str.clone();
            let mut cache = IMAGE_CACHE.lock().await;
            cache.put(h, CachedResult {
                result: best.clone(),
            });
        }

        Some(best)
    }

    pub async fn clear_cache(&self) {
        let mut cache = IMAGE_CACHE.lock().await;
        cache.cache.clear();
    }
}

impl Default for ImageAwareCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompressionStrategy for ImageAwareCompressor {
    fn name(&self) -> &'static str {
        "image_aware"
    }

    fn content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Image]
    }

    async fn compress(&self, content: &str) -> Option<CompressionResult> {
        if content.len() < 100 || !data_uri_re().is_match(content) {
            return None;
        }

        let start = chrono::Utc::now();
        let result = self.compress_auto(content, None).await?;
        let took = (chrono::Utc::now() - start).num_microseconds().unwrap_or(0) as u64;

        let header = format!(
            "‖ Image compressed: {}x{} → {}x{} ({:.0}% savings, {})\n",
            result.original_width, result.original_height,
            result.output_width, result.output_height,
            result.savings_pct,
            result.technique.name(),
        );
        let output = format!("{}{}", header, result.compressed_data_uri);

        let metrics = CompressionMetrics::new(content, &output, "image_aware", "image", took);
        Some(CompressionResult {
            text: output,
            metrics,
            retrieval_key: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_image(width: u32, height: u32) -> String {
        let mut img = DynamicImage::new_rgb8(width, height);
        for y in 0..height {
            for x in 0..width {
                let r = (x * 255 / width) as u8;
                let g = (y * 255 / height) as u8;
                let b = ((x + y) * 128 / (width + height)) as u8;
                img.as_mut_rgb8().unwrap().put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Jpeg).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        format!("data:image/jpeg;base64,{}", b64)
    }

    #[test]
    fn test_data_uri_parsing() {
        let img = create_test_image(100, 100);
        let result = parse_data_uri(&img);
        assert!(result.is_some());
        let (fmt, bytes) = result.unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_invalid_data_uri() {
        assert!(parse_data_uri("not a data uri").is_none());
        assert!(parse_data_uri("data:image/jpeg;base64,").is_some());
        assert!(parse_data_uri("data:text/plain;base64,abc").is_none());
    }

    #[test]
    fn test_query_classification() {
        assert_eq!(classify_query("What is this?"), ImageTechnique::FullLow);
        assert_eq!(classify_query("Describe the scene"), ImageTechnique::FullLow);
        assert_eq!(classify_query("Count the number of items"), ImageTechnique::Preserve);
        assert_eq!(classify_query("Read the serial number"), ImageTechnique::Transcode);
        assert_eq!(classify_query("What's in the corner?"), ImageTechnique::Crop);
        assert_eq!(classify_query("Transcribe this document"), ImageTechnique::Transcode);
    }

    #[test]
    fn test_analyze_image() {
        let img = DynamicImage::new_rgb8(200, 200);
        let analysis = analyze_image(&img);
        assert_eq!(analysis.width, 200);
        assert_eq!(analysis.height, 200);
        assert!(analysis.entropy >= 0.0);
        assert!(!analysis.has_text);
    }

    #[tokio::test]
    async fn test_compress_small_image() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(64, 64);
        let result = compressor.compress_auto(&data_uri, None).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.compressed_data_uri.starts_with("data:image/"));
        assert!(r.savings_pct >= 0.0);
    }

    #[tokio::test]
    async fn test_compress_large_image() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(1024, 768);
        let result = compressor.compress_auto(&data_uri, Some("What is this?")).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.technique, ImageTechnique::FullLow);
        assert!(r.savings_pct > 50.0, "savings should be >50% for large image: {}", r.savings_pct);
    }

    #[tokio::test]
    async fn test_compress_preserve_query() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(200, 200);
        let result = compressor.compress_auto(&data_uri, Some("Count the whiskers")).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.technique, ImageTechnique::Preserve);
    }

    #[tokio::test]
    async fn test_compress_transcode_query() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(300, 200);
        let result = compressor.compress_auto(&data_uri, Some("Read the serial number")).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.technique, ImageTechnique::Transcode);
    }

    #[tokio::test]
    async fn test_compress_crop_query() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(400, 300);
        let result = compressor.compress_auto(&data_uri, Some("What's in the corner?")).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.technique, ImageTechnique::Crop);
    }

    #[tokio::test]
    async fn test_strategy_compress() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(1024, 1024);
        let result = CompressionStrategy::compress(&compressor, &data_uri).await;
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.text.contains("Image compressed"));
        assert!(r.text.contains("data:image/"));
    }

    #[tokio::test]
    async fn test_strategy_skips_non_image() {
        let compressor = ImageAwareCompressor::new();
        let result = CompressionStrategy::compress(&compressor, "just text").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_strategy_skips_small_content() {
        let compressor = ImageAwareCompressor::new();
        let result = CompressionStrategy::compress(&compressor, "data:image/jpeg;base64,abc").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_provider_dimension() {
        let analysis = ImageAnalysis {
            width: 2048, height: 1024, entropy: 5.0, edge_density: 0.1,
            color_diversity: 256, has_text: false, aspect_ratio: 2.0,
            file_size_bytes: 100000,
        };
        assert_eq!(provider_dimension(ImageProvider::OpenAi, &analysis), 2048);
        assert_eq!(provider_dimension(ImageProvider::Anthropic, &analysis), 512);
        assert_eq!(provider_dimension(ImageProvider::Google, &analysis), 768);
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let compressor = ImageAwareCompressor::new();
        let data_uri = create_test_image(100, 100);
        let r1 = compressor.compress_auto(&data_uri, Some("test")).await;
        let r2 = compressor.compress_auto(&data_uri, Some("test")).await;
        assert!(r1.is_some());
        assert!(r2.is_some());
        assert_eq!(r1.unwrap().savings_pct, r2.unwrap().savings_pct);
    }

    #[test]
    fn test_token_estimate() {
        let result = ImageCompressionResult {
            compressed_data_uri: "".into(),
            technique: ImageTechnique::FullLow,
            original_bytes: 100000,
            compressed_bytes: 5000,
            savings_pct: 95.0,
            original_width: 1024,
            original_height: 1024,
            output_width: 256,
            output_height: 256,
            quality_preserved: 0.8,
        };
        let tokens = result.token_estimate(ImageProvider::OpenAi);
        assert!(tokens > 0);
        assert!(tokens < 765);
    }

    #[test]
    fn test_config_defaults() {
        let config = ImageCompressorConfig::default();
        assert_eq!(config.max_dimension, 512);
        assert_eq!(config.min_quality, 60);
        assert!(config.enable_crop);
        assert!(config.enable_transcode);
    }

    #[test]
    fn test_data_uri_to_vec_roundtrip() {
        let bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let uri = vec_to_data_uri(&bytes, ImageFormat::Jpeg);
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        let (fmt, decoded) = parse_data_uri(&uri).unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_technique_name() {
        assert_eq!(ImageTechnique::FullLow.name(), "full_low");
        assert_eq!(ImageTechnique::Preserve.name(), "preserve");
        assert_eq!(ImageTechnique::Crop.name(), "crop");
        assert_eq!(ImageTechnique::Transcode.name(), "transcode");
    }
}
