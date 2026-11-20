// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scene size presets, as FRAME's (`resolutions.ts`): video / broadcast,
//! IAB web banners, social and paid-ad sizes.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
    pub group: &'static str,
}

/// The groups, in FRAME's order: `(id, label)`.
pub const GROUPS: &[(&str, &str)] = &[
    ("video", "Video / broadcast"),
    ("banner", "Web banners (IAB)"),
    ("social", "Social profiles & posts"),
    ("ads", "Paid ads (Meta, Google, TikTok)"),
];

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "hd",
        label: "HD 720p",
        width: 1280,
        height: 720,
        group: "video",
    },
    Preset {
        id: "full-hd",
        label: "Full HD 1080p",
        width: 1920,
        height: 1080,
        group: "video",
    },
    Preset {
        id: "dci-2k",
        label: "DCI 2K",
        width: 2048,
        height: 1080,
        group: "video",
    },
    Preset {
        id: "uhd-4k",
        label: "UHD 4K",
        width: 3840,
        height: 2160,
        group: "video",
    },
    Preset {
        id: "vertical-full-hd",
        label: "Vertical 1080x1920",
        width: 1080,
        height: 1920,
        group: "video",
    },
    Preset {
        id: "square-1080",
        label: "Square 1080x1080",
        width: 1080,
        height: 1080,
        group: "video",
    },
    Preset {
        id: "banner-leaderboard",
        label: "Leaderboard",
        width: 728,
        height: 90,
        group: "banner",
    },
    Preset {
        id: "banner-super-leaderboard",
        label: "Super Leaderboard",
        width: 970,
        height: 90,
        group: "banner",
    },
    Preset {
        id: "banner-billboard",
        label: "Billboard",
        width: 970,
        height: 250,
        group: "banner",
    },
    Preset {
        id: "banner-medium-rectangle",
        label: "Medium Rectangle",
        width: 300,
        height: 250,
        group: "banner",
    },
    Preset {
        id: "banner-large-rectangle",
        label: "Large Rectangle",
        width: 336,
        height: 280,
        group: "banner",
    },
    Preset {
        id: "banner-half-page",
        label: "Half Page",
        width: 300,
        height: 600,
        group: "banner",
    },
    Preset {
        id: "banner-skyscraper",
        label: "Wide Skyscraper",
        width: 160,
        height: 600,
        group: "banner",
    },
    Preset {
        id: "banner-mobile-leaderboard",
        label: "Mobile Leaderboard",
        width: 320,
        height: 50,
        group: "banner",
    },
    Preset {
        id: "banner-mobile-banner",
        label: "Mobile Banner",
        width: 320,
        height: 100,
        group: "banner",
    },
    Preset {
        id: "banner-square-250",
        label: "Square 250",
        width: 250,
        height: 250,
        group: "banner",
    },
    Preset {
        id: "social-youtube-cover",
        label: "YouTube Channel Art",
        width: 2560,
        height: 1440,
        group: "social",
    },
    Preset {
        id: "social-youtube-thumbnail",
        label: "YouTube Thumbnail",
        width: 1280,
        height: 720,
        group: "social",
    },
    Preset {
        id: "social-x-header",
        label: "X / Twitter Header",
        width: 1500,
        height: 500,
        group: "social",
    },
    Preset {
        id: "social-x-post",
        label: "X / Twitter Post",
        width: 1200,
        height: 675,
        group: "social",
    },
    Preset {
        id: "social-linkedin-cover",
        label: "LinkedIn Cover",
        width: 1584,
        height: 396,
        group: "social",
    },
    Preset {
        id: "social-linkedin-post",
        label: "LinkedIn Post",
        width: 1200,
        height: 627,
        group: "social",
    },
    Preset {
        id: "social-facebook-cover",
        label: "Facebook Cover",
        width: 1640,
        height: 624,
        group: "social",
    },
    Preset {
        id: "social-instagram-post",
        label: "Instagram Post (square)",
        width: 1080,
        height: 1080,
        group: "social",
    },
    Preset {
        id: "social-instagram-portrait",
        label: "Instagram Post (4:5)",
        width: 1080,
        height: 1350,
        group: "social",
    },
    Preset {
        id: "social-instagram-story",
        label: "Instagram Story / Reels",
        width: 1080,
        height: 1920,
        group: "social",
    },
    Preset {
        id: "social-pinterest-pin",
        label: "Pinterest Pin (2:3)",
        width: 1000,
        height: 1500,
        group: "social",
    },
    Preset {
        id: "social-open-graph",
        label: "Open Graph / Link preview",
        width: 1200,
        height: 630,
        group: "social",
    },
    Preset {
        id: "ads-meta-feed-square",
        label: "Meta Feed (square)",
        width: 1080,
        height: 1080,
        group: "ads",
    },
    Preset {
        id: "ads-meta-feed-portrait",
        label: "Meta Feed (4:5)",
        width: 1080,
        height: 1350,
        group: "ads",
    },
    Preset {
        id: "ads-meta-feed-landscape",
        label: "Meta Feed (landscape)",
        width: 1200,
        height: 628,
        group: "ads",
    },
    Preset {
        id: "ads-meta-story",
        label: "Meta Story / Reels ad",
        width: 1080,
        height: 1920,
        group: "ads",
    },
    Preset {
        id: "ads-meta-carousel",
        label: "Meta Carousel",
        width: 1080,
        height: 1080,
        group: "ads",
    },
    Preset {
        id: "ads-meta-right-column",
        label: "Meta Right column",
        width: 1200,
        height: 1200,
        group: "ads",
    },
    Preset {
        id: "ads-google-responsive-landscape",
        label: "Google Display (landscape)",
        width: 1200,
        height: 628,
        group: "ads",
    },
    Preset {
        id: "ads-google-responsive-square",
        label: "Google Display (square)",
        width: 1200,
        height: 1200,
        group: "ads",
    },
    Preset {
        id: "ads-google-responsive-portrait",
        label: "Google Display (portrait)",
        width: 960,
        height: 1200,
        group: "ads",
    },
    Preset {
        id: "ads-tiktok-infeed",
        label: "TikTok In-feed",
        width: 1080,
        height: 1920,
        group: "ads",
    },
    Preset {
        id: "ads-tiktok-square",
        label: "TikTok Square",
        width: 1080,
        height: 1080,
        group: "ads",
    },
];

/// The first preset of this size (FRAME shows it as the chosen one).
pub fn find(width: f64, height: f64) -> Option<&'static Preset> {
    let (w, h) = (width.round(), height.round());
    PRESETS
        .iter()
        .find(|p| f64::from(p.width) == w && f64::from(p.height) == h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_as_frame() {
        assert_eq!(PRESETS.len(), 39);
        assert_eq!(find(1920.0, 1080.0).unwrap().id, "full-hd");
        // The first of equal sizes wins, as in FRAME.
        assert_eq!(find(1280.0, 720.0).unwrap().id, "hd");
        assert!(find(1919.0, 1080.0).is_none());
        assert!(
            PRESETS
                .iter()
                .all(|p| GROUPS.iter().any(|(g, _)| *g == p.group))
        );
    }
}
