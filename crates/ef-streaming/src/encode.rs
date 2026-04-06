//! H.264 encoding pipeline with hardware/software fallback.
//!
//! In practice, scrcpy (Android) already outputs H.264, so this module mainly
//! handles iOS transcoding and provides a unified `EncoderPipeline` abstraction.

use std::process::Stdio;
use tokio::process::Command;

/// Encoder configuration.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub hardware_accel: bool,
    /// H.264 profile (baseline, main, high).
    pub profile: H264Profile,
    /// Preset controlling speed/quality tradeoff.
    pub preset: EncoderPreset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
}

impl H264Profile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Main => "main",
            Self::High => "high",
        }
    }
}

/// Speed preset — lower latency presets produce larger output but encode faster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Fast,
    Medium,
}

impl EncoderPreset {
    pub fn as_ffmpeg_str(&self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Fast => "fast",
            Self::Medium => "medium",
        }
    }
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 30,
            bitrate_kbps: 2000,
            hardware_accel: true,
            profile: H264Profile::Baseline,
            preset: EncoderPreset::Ultrafast,
        }
    }
}

/// Detected hardware encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HwEncoder {
    /// NVIDIA NVENC
    Nvenc,
    /// Intel Quick Sync Video (VA-API)
    Vaapi,
    /// VideoToolbox (macOS)
    VideoToolbox,
    /// No hardware encoder found — use libx264
    Software,
}

impl HwEncoder {
    pub fn ffmpeg_codec(&self) -> &'static str {
        match self {
            Self::Nvenc => "h264_nvenc",
            Self::Vaapi => "h264_vaapi",
            Self::VideoToolbox => "h264_videotoolbox",
            Self::Software => "libx264",
        }
    }
}

/// Detect available hardware encoders by probing ffmpeg.
pub async fn detect_hw_encoder() -> HwEncoder {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return HwEncoder::Software,
    };

    // Prefer NVENC → VA-API → VideoToolbox → software
    if stdout.contains("h264_nvenc") {
        HwEncoder::Nvenc
    } else if stdout.contains("h264_vaapi") {
        HwEncoder::Vaapi
    } else if stdout.contains("h264_videotoolbox") {
        HwEncoder::VideoToolbox
    } else {
        HwEncoder::Software
    }
}

/// Build ffmpeg arguments for a re-encoding pipeline (used for iOS capture).
///
/// Returns the argument vector suitable for `Command::new("ffmpeg").args(...)`.
pub fn build_ffmpeg_args(cfg: &EncoderConfig, hw: &HwEncoder) -> Vec<String> {
    let codec = if cfg.hardware_accel {
        hw.ffmpeg_codec()
    } else {
        "libx264"
    };

    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(), "warning".into(),
        // Input from stdin (raw frames)
        "-f".into(), "rawvideo".into(),
        "-pix_fmt".into(), "nv21".into(),
        "-video_size".into(), format!("{}x{}", cfg.width, cfg.height),
        "-framerate".into(), cfg.fps.to_string(),
        "-i".into(), "pipe:0".into(),
        // Encoder
        "-c:v".into(), codec.into(),
    ];

    // Encoder-specific flags
    match hw {
        HwEncoder::Software | HwEncoder::Vaapi => {
            args.extend([
                "-preset".into(), cfg.preset.as_ffmpeg_str().into(),
                "-tune".into(), "zerolatency".into(),
                "-profile:v".into(), cfg.profile.as_str().into(),
            ]);
        }
        HwEncoder::Nvenc => {
            args.extend([
                "-preset".into(), "p1".into(), // lowest latency for NVENC
                "-tune".into(), "ull".into(),   // ultra low latency
                "-profile:v".into(), cfg.profile.as_str().into(),
                "-rc".into(), "cbr".into(),
            ]);
        }
        HwEncoder::VideoToolbox => {
            args.extend([
                "-realtime".into(), "1".into(),
                "-profile:v".into(), cfg.profile.as_str().into(),
            ]);
        }
    }

    args.extend([
        "-b:v".into(), format!("{}k", cfg.bitrate_kbps),
        "-maxrate".into(), format!("{}k", cfg.bitrate_kbps),
        "-bufsize".into(), format!("{}k", cfg.bitrate_kbps / 2),
        // Keyframe every 2 seconds for fast seeking / recovery
        "-g".into(), (cfg.fps * 2).to_string(),
        // Output raw H.264 Annex B to stdout
        "-f".into(), "h264".into(),
        "-an".into(),
        "pipe:1".into(),
    ]);

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_baseline_ultrafast() {
        let cfg = EncoderConfig::default();
        assert_eq!(cfg.profile, H264Profile::Baseline);
        assert_eq!(cfg.preset, EncoderPreset::Ultrafast);
        assert_eq!(cfg.fps, 30);
    }

    #[test]
    fn ffmpeg_args_contain_codec() {
        let cfg = EncoderConfig::default();
        let args = build_ffmpeg_args(&cfg, &HwEncoder::Nvenc);
        assert!(args.contains(&"h264_nvenc".to_string()));
    }

    #[test]
    fn software_fallback_uses_libx264() {
        let cfg = EncoderConfig { hardware_accel: false, ..Default::default() };
        let args = build_ffmpeg_args(&cfg, &HwEncoder::Nvenc);
        assert!(args.contains(&"libx264".to_string()));
    }
}
