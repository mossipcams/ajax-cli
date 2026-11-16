use std::{io, path::Path, process::Output};

const AGENT_IMAGE_PREFIX: &str = "ajax-agent:";
const IMAGE_FILTER: &str = "reference=ajax-agent:*";
const LIST_FORMAT: &str = "{{.Repository}}:{{.Tag}}\t{{.CreatedSince}}";
const STALE_DAYS: f64 = 7.0;

pub(crate) fn prune_old_agent_images(
    docker: &Path,
    current_tag: &str,
    run: impl Fn(&Path, &[&str], Option<&[u8]>) -> io::Result<Output>,
) {
    let Ok(listing) = run(
        docker,
        &[
            "image",
            "ls",
            "--filter",
            IMAGE_FILTER,
            "--format",
            LIST_FORMAT,
        ],
        None,
    ) else {
        return;
    };
    if !listing.status.success() {
        return;
    }
    let listing = String::from_utf8_lossy(&listing.stdout);
    for refname in stale_agent_image_refs(&listing, current_tag) {
        let _ = run(docker, &["image", "rm", &refname], None);
    }
    let _ = run(
        docker,
        &["builder", "prune", "-f", "--filter", "until=168h"],
        None,
    );
}

pub(crate) fn stale_agent_image_refs(listing: &str, current_tag: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in listing.lines() {
        let Some((refname, since)) = line.split_once('\t') else {
            continue;
        };
        if refname.is_empty() || !refname.starts_with(AGENT_IMAGE_PREFIX) || refname == current_tag
        {
            continue;
        }
        if older_than_retention(since) {
            refs.push(refname.to_string());
        }
    }
    refs
}

fn older_than_retention(since: &str) -> bool {
    let mut value = 0.0f64;
    let mut have_value = false;
    for token in since.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if matches!(lower.as_str(), "about" | "an" | "a") {
            continue;
        }
        if !have_value {
            match lower.parse::<f64>() {
                Ok(parsed) => {
                    value = parsed;
                    have_value = true;
                }
                Err(_) => continue,
            }
            continue;
        }
        return match lower.trim_end_matches('s') {
            "day" => value > STALE_DAYS,
            "week" | "month" | "year" => true,
            _ => false,
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: &str = "ajax-agent:bbbbbbbbbbbbbbbb";

    fn listing() -> String {
        [
            "REPOSITORY\tTAG",
            "ajax-agent:aaaaaaaaaaaaaaaa\t8 days ago",
            "ajax-agent:cccccccccccccccc\t1 day ago",
            "ajax-agent:dddddddddddddddd\t2 weeks ago",
            "ajax-agent:eeeeeeeeeeeeeeee\tAbout an hour ago",
            "ajax-agent:ffffffffffffffff\t6 weeks ago",
        ]
        .join("\n")
    }

    #[test]
    fn stale_selection_skips_the_current_and_recent_tags() {
        let stale = stale_agent_image_refs(&listing(), CURRENT);
        assert_eq!(
            stale,
            vec![
                "ajax-agent:aaaaaaaaaaaaaaaa",
                "ajax-agent:dddddddddddddddd",
                "ajax-agent:ffffffffffffffff"
            ]
        );
    }

    #[test]
    fn the_current_tag_is_never_selected_even_when_old() {
        let stale = stale_agent_image_refs("ajax-agent:bbbbbbbbbbbbbbbb\t30 days ago\n", CURRENT);
        assert!(stale.is_empty());
    }

    #[test]
    fn a_seven_day_boundary_age_is_kept() {
        assert!(
            stale_agent_image_refs("ajax-agent:aaaaaaaaaaaaaaaa\t7 days ago\n", CURRENT).is_empty()
        );
        assert_eq!(
            stale_agent_image_refs("ajax-agent:aaaaaaaaaaaaaaaa\t8 days ago\n", CURRENT),
            vec!["ajax-agent:aaaaaaaaaaaaaaaa"]
        );
    }

    #[test]
    fn non_agent_images_and_malformed_lines_are_ignored() {
        let listing = "redis:7\t2 weeks ago\n\tnot-a-line\nimage without tab\t30 days ago\n";
        assert!(stale_agent_image_refs(listing, CURRENT).is_empty());
    }
}
