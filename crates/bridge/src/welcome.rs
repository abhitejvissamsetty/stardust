//! The welcome page shown once after Stardust is first opened: the sites a creator might
//! want one click away.

/// A platform the welcome page offers, with the page its bookmark opens. The page picks
/// platforms by `id` only, so it can't make the browser bookmark an address of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

pub const PLATFORMS: [Platform; 10] = [
    Platform {
        id: "youtube",
        name: "YouTube Studio",
        url: "https://studio.youtube.com/",
    },
    Platform {
        id: "instagram",
        name: "Instagram",
        url: "https://www.instagram.com/",
    },
    Platform {
        id: "tiktok",
        name: "TikTok Studio",
        url: "https://www.tiktok.com/tiktokstudio",
    },
    Platform {
        id: "meta",
        name: "Meta Business Suite",
        url: "https://business.facebook.com/",
    },
    Platform {
        id: "x",
        name: "X",
        url: "https://x.com/",
    },
    Platform {
        id: "linkedin",
        name: "LinkedIn",
        url: "https://www.linkedin.com/",
    },
    Platform {
        id: "twitch",
        name: "Twitch Dashboard",
        url: "https://dashboard.twitch.tv/",
    },
    Platform {
        id: "pinterest",
        name: "Pinterest",
        url: "https://www.pinterest.com/",
    },
    Platform {
        id: "canva",
        name: "Canva",
        url: "https://www.canva.com/",
    },
    Platform {
        id: "capcut",
        name: "CapCut",
        url: "https://www.capcut.com/",
    },
];

/// The platforms named by `ids`, in the order of the list above, each once. Unknown names
/// are ignored.
pub fn chosen_platforms(ids: &[String]) -> Vec<Platform> {
    PLATFORMS
        .into_iter()
        .filter(|platform| ids.iter().any(|id| id == platform.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_platform_is_a_secure_web_address_with_its_own_id() {
        let mut ids = HashSet::new();
        for platform in PLATFORMS {
            assert!(platform.url.starts_with("https://"), "{}", platform.id);
            assert!(ids.insert(platform.id), "{} is repeated", platform.id);
            assert!(!platform.name.is_empty());
        }
    }

    #[test]
    fn only_known_platforms_are_chosen_once_each_in_list_order() {
        let ids: Vec<String> = ["tiktok", "nonsense", "youtube", "tiktok", "javascript:1"]
            .map(String::from)
            .to_vec();
        let chosen: Vec<&str> = chosen_platforms(&ids).iter().map(|p| p.id).collect();
        assert_eq!(chosen, ["youtube", "tiktok"]);
        assert!(chosen_platforms(&[]).is_empty());
    }
}
