//! Cookie-authenticated intra.42.fr endpoints: the holy-graph project data,
//! user search, cluster occupancy and the `/{slug}/mine` project page
//! (attachments, team, git repository).

use std::time::Duration;

use serde::de::DeserializeOwned;

use super::{Api, ApiError, ApiResult};

const TTL_GRAPH: Duration = Duration::from_secs(30 * 60);
const TTL_CLUSTERS: Duration = Duration::from_secs(60);
const TTL_MINE: Duration = Duration::from_secs(10 * 60);

impl Api {
    /// GET an intra web endpoint with the Rails session cookie, transparently
    /// re-bootstrapping the session once when it has expired. The intranet
    /// answers 401 JSON or a login-page redirect for dead sessions.
    async fn web_get_json<T: DeserializeOwned>(&self, url: &str) -> ApiResult<T> {
        let fetch = || async {
            let resp = self.http.get(url).send().await?;
            let status = resp.status();
            if status.as_u16() == 401 {
                return Err(ApiError::SessionExpired);
            }
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            if !status.is_success() {
                return Err(ApiError::from_response("intra web", resp).await);
            }
            if !content_type.contains("json") {
                // We were bounced to an HTML login page: session is dead.
                return Err(ApiError::SessionExpired);
            }
            resp.json::<T>().await.map_err(|error| ApiError::Parse {
                endpoint: "intra web",
                detail: error.to_string(),
            })
        };
        match fetch().await {
            Ok(value) => Ok(value),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }

    /// All cursus projects with per-user state (holy graph data).
    pub async fn project_data(
        &self,
        cursus_id: u32,
        campus_id: u32,
        fresh: bool,
    ) -> ApiResult<Vec<super::ProjectDataEntry>> {
        let key = format!("project_data/{cursus_id}/{campus_id}");
        if !fresh && let Some(cached) = self.cache.get(&key, TTL_GRAPH) {
            return Ok(cached);
        }
        let entries: Vec<super::ProjectDataEntry> = self
            .web_get_json(&format!(
                "{}/project_data.json?cursus_id={cursus_id}&campus_id={campus_id}",
                super::PROJECTS_BASE
            ))
            .await?;
        self.cache.put(&key, &entries);
        Ok(entries)
    }

    /// Login-prefix user search (max ~5 hits, same as the web UI).
    pub async fn search_users(&self, query: &str) -> ApiResult<Vec<super::SearchResult>> {
        let url = format!(
            "{}/searches/search.json?query={}",
            super::PROFILE_BASE,
            urlencoding_lite(query)
        );
        self.web_get_json(&url).await
    }

    /// Currently occupied cluster seats across campuses.
    pub async fn cluster_seats(&self, fresh: bool) -> ApiResult<Vec<super::ClusterSeat>> {
        if !fresh && let Some(cached) = self.cache.get("clusters", TTL_CLUSTERS) {
            return Ok(cached);
        }
        let seats: Vec<super::ClusterSeat> = self
            .web_get_json(&format!("{}/clusters.json", super::META_BASE))
            .await?;
        self.cache.put("clusters", &seats);
        Ok(seats)
    }

    /// Scrape `projects.intra.42.fr/{slug}/mine` for team + attachments.
    pub async fn project_mine(&self, slug: &str, fresh: bool) -> ApiResult<super::ProjectMine> {
        let key = format!("mine/{slug}");
        if !fresh && let Some(cached) = self.cache.get(&key, TTL_MINE) {
            return Ok(cached);
        }
        let fetch = || async {
            let resp = self
                .http
                .get(format!("{}/{slug}/mine", super::PROJECTS_BASE))
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("project mine", resp).await);
            }
            let html = resp.text().await.unwrap_or_default();
            parse_project_mine(&html).ok_or_else(|| ApiError::Parse {
                endpoint: "project mine",
                detail: format!("could not parse `{slug}`"),
            })
        };
        let mine = match fetch().await {
            Ok(mine) => mine,
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await?
                } else {
                    return Err(ApiError::SessionExpired);
                }
            }
            Err(error) => return Err(error),
        };
        self.cache.put(&key, &mine);
        Ok(mine)
    }

    /// Download a CDN document (subjects, archives) into the downloads dir
    /// and return the destination path.
    pub async fn download_attachment(&self, url: &str, name: &str) -> ApiResult<String> {
        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ApiError::from_response("cdn", resp).await);
        }
        let bytes = resp.bytes().await?;
        let dir = crate::config::downloads_dir();
        let safe_name = name.replace(['/', '\\'], "_");
        let path = dir.join(safe_name);
        std::fs::write(&path, &bytes)
            .map_err(|error| ApiError::Other(format!("write {}: {error}", path.display())))?;
        Ok(path.display().to_string())
    }
}

fn parse_project_mine(html: &str) -> Option<super::ProjectMine> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);

    let mut mine = super::ProjectMine {
        status: select_attr(&document, ".project-status-box", "data-status"),
        ..Default::default()
    };

    if let Ok(selector) = Selector::parse(".team-name") {
        mine.team_name = document
            .select(&selector)
            .next()
            .map(|element| element.text().collect::<String>().trim().to_owned());
    }

    if let (Ok(selector), Ok(user_selector)) = (
        Selector::parse(".team-content"),
        Selector::parse(r#"a[href*="profile.intra.42.fr/users/"]"#),
    ) {
        for team in document.select(&selector) {
            for link in team.select(&user_selector) {
                if let Some(href) = link.attr("href")
                    && let Some(login) = href.rsplit('/').next()
                    && !login.is_empty()
                    && !mine.members.iter().any(|m| m == login)
                {
                    mine.members.push(login.to_owned());
                }
            }
        }
    }

    if let Ok(selector) = Selector::parse(".team-repo input.form-control") {
        mine.git_repo = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("value"))
            .map(str::to_owned);
    }

    if let Ok(selector) = Selector::parse("span[data-long-date]") {
        mine.locked_at = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("data-long-date"))
            .map(str::to_owned);
    }

    if let (Ok(selector), name_selector) = (
        Selector::parse(".project-attachment-item"),
        Selector::parse(".attachment-name a").expect("static selector"),
    ) {
        for item in document.select(&selector) {
            if let Some(link) = item.select(&name_selector).next()
                && let Some(href) = link.attr("href")
            {
                let name = link.text().collect::<String>().trim().to_owned();
                if !name.is_empty() {
                    mine.attachments.push(super::Attachment {
                        name,
                        url: href.to_owned(),
                    });
                }
            }
        }
    }

    if let Ok(selector) = Selector::parse(r#"a[data-method="delete"][href*="projects_users"]"#) {
        mine.unsub_url = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("href"))
            .map(str::to_owned);
    }

    Some(mine)
}

fn select_attr(document: &scraper::Html, selector: &str, attr: &str) -> Option<String> {
    use scraper::Selector;
    Selector::parse(selector)
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .next()
                .and_then(|element| element.attr(attr))
        })
        .map(str::to_owned)
}

/// Percent-encode a query value without pulling in a whole crate for it.
fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
