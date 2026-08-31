//! Command -> Msg dispatch running on the async runtime. One command may
//! fan out into several parallel requests; each answer is its own message
//! so the UI can render partial data as it arrives.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use crate::api::models::*;
use crate::api::{Api, ApiError, auth};
use crate::bus::{Command, Msg};

pub async fn dispatch(api: Arc<Api>, tx: Sender<Msg>, command: Command) {
    match command {
        Command::Login { username, password } => {
            let result = auth::login(&api, &username, &password).await;
            if result.is_ok() {
                preload_dashboard(&api, &tx, false).await;
            }
            let _ = tx.send(Msg::Login(result));
        }

        Command::Logout => {
            auth::logout(&api).await;
            let _ = tx.send(Msg::LoggedOut);
        }

        Command::Restore(stored) => restore(api, tx, stored).await,

        Command::LoadDashboard { fresh } => preload_dashboard(&api, &tx, fresh).await,

        Command::LoadEvalForm { id } => {
            spawn_job(&api, &tx, id, |api, id| async move {
                Msg::EvalFormLoaded {
                    id,
                    result: api.evaluation_form(id).await,
                }
            });
        }

        Command::SubmitEval(submission) => {
            spawn_job(&api, &tx, submission, |api, submission| async move {
                let id = submission.scale_team_id;
                Msg::EvalSubmitted {
                    id,
                    result: api.submit_evaluation(&submission).await,
                }
            });
        }

        Command::LoadProjects { fresh } => {
            spawn_job(&api, &tx, fresh, |api, fresh| async move {
                Msg::ProjectData(dashboard_graph(&api, fresh).await)
            });
            spawn_job(&api, &tx, (), |api, ()| async move {
                Msg::Ongoing(ongoing_of(&api).await)
            });
            spawn_job(&api, &tx, (), |api, ()| async move {
                Msg::Marked(marked_of(&api).await)
            });
        }

        Command::LoadMine { slug, fresh } => {
            let result = api.project_mine(&slug, fresh).await;
            let _ = tx.send(Msg::Mine { slug, result });
        }

        Command::LoadSchedule { slug, fresh } => {
            let result = api.project_schedule(&slug, fresh).await;
            let _ = tx.send(Msg::Schedule { slug, result });
        }

        Command::DownloadAttachment { project, name, url } => {
            let result = api.download_attachment(&url, &name, &project).await;
            let _ = tx.send(Msg::DownloadDone { name, result });
        }

        Command::LoadSubject { slug, url } => {
            let result = api.subject_markdown(&slug, &url).await;
            let _ = tx.send(Msg::SubjectLoaded { slug, result });
        }

        Command::CloneRepo {
            slug,
            repo,
            dest,
            name,
            passphrase,
        } => {
            match git_clone(&repo, &dest, name.as_deref(), passphrase.as_deref()).await {
                Ok(path) => {
                    let _ = tx.send(Msg::CloneDone {
                        slug,
                        path: path.clone(),
                        result: Ok(path),
                    });
                }
                // A locked ssh key on the first attempt: hand the clone
                // back to the UI, which asks for the passphrase in a popup.
                Err(CloneFailure::Auth(_)) if passphrase.is_none() => {
                    let _ = tx.send(Msg::CloneNeedsPassphrase {
                        slug,
                        repo,
                        dest,
                        name,
                    });
                }
                Err(CloneFailure::Auth(tail)) => {
                    let _ = tx.send(Msg::CloneDone {
                        slug,
                        path: dest,
                        result: Err(format!("{tail} — wrong passphrase?")),
                    });
                }
                Err(CloneFailure::Other(tail)) => {
                    let _ = tx.send(Msg::CloneDone {
                        slug,
                        path: dest,
                        result: Err(tail),
                    });
                }
            }
        }

        Command::Search { query } => {
            let result = api.search_users(&query).await;
            let _ = tx.send(Msg::SearchResults(result));
        }

        Command::LoadUser { login } => load_user(api, tx, login).await,
        Command::LoadUserMine { login, slug, fresh } => {
            let result = api.project_mine_for_user(&slug, &login, fresh).await;
            let _ = tx.send(Msg::UserMine {
                login,
                slug,
                result,
            });
        }

        Command::LoadEvent { id } => {
            let result = api.event_detail(id).await;
            let _ = tx.send(Msg::EventDetail { id, result });
        }

        Command::SetEventSubscription {
            id,
            url,
            csrf_token,
            subscribe,
        } => {
            let result = api
                .set_event_subscription(&url, &csrf_token, subscribe)
                .await;
            if result.is_ok() {
                // Refresh the popup so counts and the footer action match.
                let fresh = api.event_detail(id).await;
                let _ = tx.send(Msg::EventDetail { id, result: fresh });
            }
            let _ = tx.send(Msg::EventWrite { subscribe, result });
        }

        Command::LoadClusters { fresh } => {
            let result = api.cluster_seats(fresh).await;
            let _ = tx.send(Msg::Clusters(result));
        }

        Command::LoadSlotsOverview { anchor } => {
            spawn_job(&api, &tx, (), |api, ()| async move {
                Msg::SlotsProjects(api.slots_projects().await)
            });
            spawn_job(&api, &tx, anchor, |api, anchor| async move {
                Msg::ReservedSlots(api.reserved_slots(anchor, 21).await)
            });
            let result = api.open_slots(anchor, 21).await;
            let _ = tx.send(Msg::OpenSlots(result));
        }

        Command::SyncSlotsProjects => {
            let result = api.slots_sync_projects().await;
            if result.is_ok() {
                let projects = api.slots_projects().await;
                let _ = tx.send(Msg::SlotsProjects(projects));
            }
            let _ = tx.send(Msg::SlotsSynced(result));
        }

        Command::LoadProjectSlots {
            ps_id,
            anchor,
            campus,
            remote,
        } => {
            let result = api.project_slots(ps_id, anchor, 21, &campus, remote).await;
            let _ = tx.send(Msg::ProjectSlots(result));
        }

        Command::CreateSlot {
            begin,
            end,
            campus,
            remote,
        } => {
            let result = api.create_open_slot(begin, end, &campus, remote).await;
            let _ = tx.send(Msg::SlotWrite(result));
        }

        Command::DeleteSlot { start, end } => {
            let result = api.delete_open_slot(start, end).await;
            let _ = tx.send(Msg::SlotWrite(result));
        }

        Command::BookSlot {
            ps_id,
            time,
            campus,
        } => {
            let result = api.book_project_slot(ps_id, &time, &campus).await;
            let _ = tx.send(Msg::SlotWrite(result));
        }
    }
}

/// Run `job` as its own tokio task, forwarding its `Msg` back to the UI.
fn spawn_job<C, F, Fut>(api: &Arc<Api>, tx: &Sender<Msg>, ctx: C, job: F)
where
    C: Send + 'static,
    F: FnOnce(Arc<Api>, C) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Msg> + Send + 'static,
{
    let api = Arc::clone(api);
    let tx = tx.clone();
    tokio::spawn(async move {
        let msg = job(api, ctx).await;
        let _ = tx.send(msg);
    });
}

/// Rehydrate a persisted session (tokens + cookies) and verify it.
/// If the token is expired and refresh fails, try auto-login with stored
/// username/password from the encrypted vault.
async fn restore(api: Arc<Api>, tx: Sender<Msg>, stored: crate::config::StoredSession) {
    let login = stored.login.clone();
    let now = chrono::Utc::now().timestamp();
    api.set_tokens(crate::api::TokenSet {
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
        access_expires_at: stored.access_expires_at,
    })
    .await;

    let mut ok = true;
    let mut refreshed = true;
    if stored.access_expires_at - now < 30 && api.refresh().await.is_err() {
        // Try auto-login via stored credentials
        if let Some(vault) = crate::secure::load_vault() {
            if !vault.username.is_empty() && !vault.password.is_empty() {
                match auth::login(&api, &vault.username, &vault.password).await {
                    Ok(outcome) => {
                        // Credentials good — treat as restored.
                        // Update login to outcome in case it changed
                        let _ = tx.send(Msg::SessionRestored {
                            login: outcome.login.clone(),
                            ok: true,
                        });
                        // Bootstrap sessions
                        let api_bg = Arc::clone(&api);
                        tokio::spawn(async move {
                            if !api_bg.has_intra_session() {
                                auth::bootstrap_intra_session(&api_bg).await;
                            }
                            if !api_bg.has_slots_session() {
                                auth::bootstrap_slots_session(&api_bg).await;
                            }
                            auth::persist_session(&api_bg).await;
                        });
                        preload_dashboard(&api, &tx, false).await;
                        return;
                    }
                    Err(_) => {
                        refreshed = false;
                        ok = false;
                    }
                }
            } else {
                refreshed = false;
                ok = false;
            }
        } else {
            refreshed = false;
            ok = false;
        }
    }
    if ok {
        match api.me_summary(false).await {
            Ok(_) => {
                // Re-validate web sessions in the background, off the hot path.
                let api_bg = Arc::clone(&api);
                tokio::spawn(async move {
                    if !api_bg.has_intra_session() {
                        auth::bootstrap_intra_session(&api_bg).await;
                    }
                    if !api_bg.has_slots_session() {
                        auth::bootstrap_slots_session(&api_bg).await;
                    }
                    auth::persist_session(&api_bg).await;
                });
                preload_dashboard(&api, &tx, false).await;
            }
            Err(_) => {
                // me_summary failed — maybe token revoked, try credentials once
                if refreshed
                    && let Some(vault) = crate::secure::load_vault()
                    && !vault.username.is_empty()
                    && !vault.password.is_empty()
                    && auth::login(&api, &vault.username, &vault.password)
                        .await
                        .is_ok()
                {
                    let api_bg = Arc::clone(&api);
                    tokio::spawn(async move {
                        if !api_bg.has_intra_session() {
                            auth::bootstrap_intra_session(&api_bg).await;
                        }
                        if !api_bg.has_slots_session() {
                            auth::bootstrap_slots_session(&api_bg).await;
                        }
                        auth::persist_session(&api_bg).await;
                    });
                    preload_dashboard(&api, &tx, false).await;
                    let vault2 = crate::secure::load_vault();
                    let new_login = vault2.map(|v| v.login).unwrap_or(login.clone());
                    let _ = tx.send(Msg::SessionRestored {
                        login: new_login,
                        ok: true,
                    });
                    return;
                }
                ok = false;
            }
        }
    }
    let _ = tx.send(Msg::SessionRestored { login, ok });
}

/// Fire every dashboard request concurrently; each result is its own Msg.
async fn preload_dashboard(api: &Arc<Api>, tx: &Sender<Msg>, fresh: bool) {
    let login = me_login(api).await;

    spawn_job(api, tx, fresh, |api, fresh| async move {
        Msg::MeSummary(api.me_summary(fresh).await)
    });
    spawn_job(
        api,
        tx,
        (fresh, login.clone()),
        |api, (fresh, login)| async move { Msg::MyProfile(api.user_profile(&login, fresh).await) },
    );
    spawn_job(api, tx, fresh, |api, fresh| async move {
        Msg::MyEvents(api.my_events(fresh).await)
    });
    spawn_job(api, tx, (), |api, ()| async move {
        Msg::MyScaleTeams(api.my_scale_teams().await)
    });
    spawn_job(
        api,
        tx,
        (fresh, login.clone()),
        |api, (fresh, login)| async move { Msg::MyLogtime(api.locations_stats(&login, fresh).await) },
    );
    spawn_job(api, tx, (), |api, ()| async move {
        Msg::MyNotifications(api.my_notifications().await)
    });

    // Booked open hours: students reserve evaluation slots on my open
    // hours (slots.42belgium.be). Those duties never show up in
    // `/users/me/scale_teams` until they materialize shortly before they
    // begin, so derive the upcoming ones from the slot feeds: every
    // reserved booking that overlaps one of my open hours.
    spawn_job(api, tx, (), |api, ()| async move {
        let parse = crate::api::slots::parse_slot_time;
        let result = async {
            let anchor = chrono::Local::now().date_naive();
            let open = api.open_slots(anchor, 14).await?;
            let reserved = api.reserved_slots(anchor, 14).await?;
            let booked = reserved
                .into_iter()
                .filter(|slot| {
                    let (Some(start), Some(end)) = (
                        slot.start.as_deref().and_then(parse),
                        slot.end.as_deref().and_then(parse),
                    ) else {
                        return false;
                    };
                    open.iter().any(|hour| {
                        matches!(
                            (
                                hour.start.as_deref().and_then(parse),
                                hour.end.as_deref().and_then(parse),
                            ),
                            (Some(hour_start), Some(hour_end)) if hour_start < end && start < hour_end
                        )
                    })
                })
                .collect();
            Ok::<Vec<Slot>, ApiError>(booked)
        }
        .await;
        Msg::MyEvalSlots(result)
    });

    // Fetches that need the numeric id wait for `me` first.
    let me = api.me_summary(fresh).await;
    let Ok(me) = me else { return };
    let Some(id) = me.id else { return };
    spawn_job(api, tx, fresh, move |api, fresh| async move {
        Msg::MyCursus(api.user_cursus(id, fresh).await)
    });
    spawn_job(api, tx, (), move |api, ()| async move {
        Msg::MyAchievements(api.user_achievements(id).await)
    });
    spawn_job(api, tx, (), move |api, ()| async move {
        Msg::MyPace(api.pace_profile(id).await)
    });
    spawn_job(api, tx, (), move |api, ()| async move {
        Msg::MyAttendance(api.attendance_summary(id).await)
    });
    spawn_job(api, tx, (), move |api, ()| async move {
        Msg::MyCampus(api.user_campus(id).await)
    });
    if let Some(login) = me.login {
        spawn_job(api, tx, (), move |api, ()| async move {
            Msg::Marked(api.marked_projects(&login, 21).await)
        });
    }
}

/// Fetch everything needed to render another student's profile.
async fn load_user(api: Arc<Api>, tx: Sender<Msg>, login: String) {
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = api.user_profile(&login, false).await.map(Box::new);
        Msg::UserView { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = match api.user_profile(&login, false).await {
            Ok(profile) if profile.id.is_some() => {
                api.user_cursus(profile.id.unwrap_or_default(), false).await
            }
            Ok(_) => Err(ApiError::Other("user has no id".into())),
            Err(error) => Err(error),
        };
        Msg::UserCursus { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = match api.user_profile(&login, false).await {
            Ok(profile) if profile.id.is_some() => {
                api.user_achievements(profile.id.unwrap_or_default()).await
            }
            Ok(_) => Err(ApiError::Other("user has no id".into())),
            Err(error) => Err(error),
        };
        Msg::UserAchievements { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = api.locations_stats(&login, false).await;
        Msg::UserLogtime { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let patroning = api.user_patroning(&login).await;
        let patroned = api.user_patroned(&login).await;
        Msg::UserPatrons {
            login,
            patroning,
            patroned,
        }
    });
    // Extra info matching dashboard: marked/ongoing + pace + campus
    // For Piscine students (e.g. mtorfs, cursus 65 `c-piscine-antwerp`)
    // profile-v3 requests `?cursus_id=65` not `21`. We therefore resolve
    // the user's actual cursus list first and fetch for every cursus id,
    // merging results (covers users with both Piscine + 42cursus).
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = async {
            let profile = api.user_profile(&login, false).await?;
            let id = profile
                .id
                .ok_or_else(|| ApiError::Other("user has no id".into()))?;
            let cursus_list = api.user_cursus(id, false).await.unwrap_or_default();
            let ids: Vec<u32> = if cursus_list.is_empty() {
                vec![21, 65]
            } else {
                cursus_list.iter().filter_map(|c| c.id).collect()
            };
            let mut merged: Vec<MarkedProject> = Vec::new();
            for cid in ids {
                if let Ok(mut v) = api.marked_projects(&login, cid).await {
                    merged.append(&mut v);
                }
            }
            // Deduplicate by slug when same project appears in multiple cursus
            let mut seen = std::collections::HashSet::new();
            merged.retain(|p| {
                let key = p.display_slug().unwrap_or("").to_owned()
                    + &p.final_mark.unwrap_or(0).to_string();
                seen.insert(key)
            });
            Ok::<Vec<MarkedProject>, ApiError>(merged)
        }
        .await;
        Msg::UserMarked { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = async {
            let profile = api.user_profile(&login, false).await?;
            let id = profile
                .id
                .ok_or_else(|| ApiError::Other("user has no id".into()))?;
            let cursus_list = api.user_cursus(id, false).await.unwrap_or_default();
            let ids: Vec<u32> = if cursus_list.is_empty() {
                vec![21, 65]
            } else {
                cursus_list.iter().filter_map(|c| c.id).collect()
            };
            let mut merged: Vec<OngoingProject> = Vec::new();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for cid in ids {
                if let Ok(mut v) = api.ongoing_projects(id, cid).await {
                    for p in v.drain(..) {
                        if let Some(slug) = &p.project_slug {
                            if !seen.insert(slug.clone()) {
                                continue;
                            }
                        }
                        merged.push(p);
                    }
                }
            }
            Ok::<Vec<OngoingProject>, ApiError>(merged)
        }
        .await;
        Msg::UserOngoing { login, result }
    });
    spawn_job(&api, &tx, login.clone(), |api, login| async move {
        let result = match api.user_profile(&login, false).await {
            Ok(profile) if profile.id.is_some() => {
                let id = profile.id.unwrap_or_default();
                api.pace_profile(id).await
            }
            Ok(_) => Err(ApiError::Other("user has no id".into())),
            Err(error) => Err(error),
        };
        Msg::UserPace { login, result }
    });
    spawn_job(&api, &tx, login, |api, login| async move {
        let result = match api.user_profile(&login, false).await {
            Ok(profile) if profile.id.is_some() => {
                api.user_campus(profile.id.unwrap_or_default()).await
            }
            Ok(_) => Err(ApiError::Other("user has no id".into())),
            Err(error) => Err(error),
        };
        Msg::UserCampus { login, result }
    });
}

async fn me_login(api: &Arc<Api>) -> String {
    api.me_summary(false)
        .await
        .ok()
        .and_then(|me| me.login)
        .unwrap_or_default()
}

async fn graph_context(api: &Arc<Api>) -> (u32, u32) {
    let me = api.me_summary(false).await.ok();
    let id = me.as_ref().and_then(|me| me.id).unwrap_or_default();
    let cursus = api
        .user_cursus(id, false)
        .await
        .ok()
        .and_then(|list| {
            list.iter()
                .find(|cursus| cursus.slug.as_deref() == Some("42cursus"))
                .or_else(|| list.first())
                .and_then(|cursus| cursus.id)
        })
        .unwrap_or(21);
    let campus = api
        .user_campus(id)
        .await
        .ok()
        .and_then(|list| {
            list.iter()
                .find(|campus| campus.is_primary)
                .and_then(|campus| campus.id)
        })
        .unwrap_or(12);
    (cursus, campus)
}

async fn dashboard_graph(api: &Arc<Api>, fresh: bool) -> Result<Vec<ProjectDataEntry>, ApiError> {
    let (cursus, campus) = graph_context(api).await;
    api.project_data(cursus, campus, fresh).await
}

async fn ongoing_of(api: &Arc<Api>) -> Result<Vec<OngoingProject>, ApiError> {
    let (cursus, _) = graph_context(api).await;
    let id = api
        .me_summary(false)
        .await
        .ok()
        .and_then(|me| me.id)
        .unwrap_or_default();
    api.ongoing_projects(id, cursus).await
}

async fn marked_of(api: &Arc<Api>) -> Result<Vec<MarkedProject>, ApiError> {
    let login = me_login(api).await;
    if login.is_empty() {
        return Ok(Vec::new());
    }
    // Piscine students (e.g. mtorfs) have cursus 65, not 21 — use the
    // actual primary cursus from the user's cursus list.
    let (cursus, _) = graph_context(api).await;
    let me = api.me_summary(false).await.ok().and_then(|m| m.id);
    if let Some(id) = me {
        if let Ok(list) = api.user_cursus(id, false).await {
            let ids: Vec<u32> = list.iter().filter_map(|c| c.id).collect();
            if !ids.is_empty() {
                let mut merged = Vec::new();
                for cid in &ids {
                    if let Ok(mut v) = api.marked_projects(&login, *cid).await {
                        merged.append(&mut v);
                    }
                }
                if !merged.is_empty() {
                    let mut seen = std::collections::HashSet::new();
                    merged.retain(|p| {
                        p.display_slug()
                            .map(|s| seen.insert(s.to_owned()))
                            .unwrap_or(true)
                    });
                    return Ok(merged);
                }
            }
        }
    }
    api.marked_projects(&login, cursus).await
}

/// Why a clone failed: an ssh/credential rejection (the passphrase-popup
/// case) or anything else.
enum CloneFailure {
    Auth(String),
    Other(String),
}

/// `git clone` into `dest`, optionally under an explicit folder name —
/// without one git picks the repo's own name, exactly like on the command
/// line. Returns the resulting folder. Vogsphere needs the campus network +
/// SSH key, so failures surface as the last lines of git's output instead
/// of a raw io error.
///
/// The TUI owns the terminal: ssh prompts (passphrase, host key) write
/// straight to /dev/tty, drawing over the UI while the event loop eats the
/// keystrokes ssh waits on. So prompting is always disabled and the clone
/// runs non-interactively; a passphrase-protected key bounces the first
/// attempt, the UI asks for the passphrase in a popup and this runs again
/// with `passphrase` set — ssh then reads it from a throwaway askpass
/// shim, never from the terminal.
async fn git_clone(
    repo: &str,
    dest: &str,
    name: Option<&str>,
    passphrase: Option<&str>,
) -> Result<String, CloneFailure> {
    std::fs::create_dir_all(dest)
        .map_err(|error| CloneFailure::Other(format!("cannot create {dest}: {error}")))?;
    let mut command = tokio::process::Command::new("git");
    command.arg("clone").arg(repo).current_dir(dest);
    if let Some(name) = name {
        command.arg(name);
    }
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true")
        .env(
            "GIT_SSH_COMMAND",
            if passphrase.is_some() {
                // The askpass shim answers the passphrase; unknown vogsphere
                // host keys are trusted on first use, changed ones abort.
                "ssh -oStrictHostKeyChecking=accept-new"
            } else {
                // BatchMode forbids every prompt outright — an `ssh-add`ed
                // key still works, the agent answers without a terminal.
                "ssh -oBatchMode=yes -oStrictHostKeyChecking=accept-new"
            },
        );
    let askpass = match passphrase.map(askpass_shim) {
        Some(Ok(guard)) => Some(guard),
        Some(Err(error)) => return Err(CloneFailure::Other(error)),
        None => None,
    };
    if let Some(guard) = &askpass {
        command
            .env("SSH_ASKPASS", &guard.shim)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", ":0");
    }
    let output = match command.output().await {
        Ok(output) => output,
        Err(error) => {
            return Err(CloneFailure::Other(format!("cannot run git: {error}")));
        }
    };
    // The guard would clean up on drop anyway, but the secret should not
    // outlive the clone a second longer than it must.
    drop(askpass);
    if output.status.success() {
        let folder = name
            .map(str::to_owned)
            .unwrap_or_else(|| repo_default_folder(repo));
        return Ok(std::path::Path::new(dest)
            .join(folder)
            .display()
            .to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().filter(|line| !line.is_empty()).collect();
    let tail = tail
        .iter()
        .rev()
        .take(3)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    let message = if tail.is_empty() {
        format!("git exited with {}", output.status)
    } else {
        tail
    };
    if auth_failure(&stderr) {
        Err(CloneFailure::Auth(message))
    } else {
        Err(CloneFailure::Other(message))
    }
}

/// True when git's stderr reads like an ssh/credential rejection — the
/// passphrase-popup case — rather than a network or repository error.
fn auth_failure(stderr: &str) -> bool {
    stderr.contains("Permission denied")
        || stderr.contains("publickey")
        || stderr.contains("passphrase")
        || stderr.contains("Authentication failed")
}

/// The askpass pair: `secret` holds the passphrase (0600) and `shim` is a
/// 0700 script printing it — `SSH_ASKPASS` demands an executable, so the
/// passphrase rides through a file pair in the runtime dir instead of any
/// terminal. Both are removed the moment the guard drops.
struct AskpassGuard {
    secret: std::path::PathBuf,
    shim: std::path::PathBuf,
}

impl Drop for AskpassGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.secret);
        let _ = std::fs::remove_file(&self.shim);
    }
}

fn askpass_shim(passphrase: &str) -> Result<AskpassGuard, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let stem = format!("42cli-askpass-{}", std::process::id());
    let secret = dir.join(format!("{stem}.secret"));
    let shim = dir.join(format!("{stem}.sh"));
    std::fs::write(&secret, passphrase)
        .map_err(|error| format!("cannot write askpass secret: {error}"))?;
    // Single-quote the path; the only escape that can occur is a quote.
    let quoted = secret.display().to_string().replace('\'', "'\\''");
    let written = std::fs::write(&shim, format!("#!/bin/sh\ncat '{quoted}'\n")).map_err(|error| {
        let _ = std::fs::remove_file(&secret);
        format!("cannot write askpass shim: {error}")
    });
    written?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600));
        let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700));
    }
    Ok(AskpassGuard { secret, shim })
}

/// git's default folder: the basename of an URL or SCP-style path, with a
/// trailing `.git` stripped — `git@host:x/y/name.git` -> `name`.
fn repo_default_folder(repo: &str) -> String {
    repo.rsplit(['/', ':'])
        .next()
        .unwrap_or(repo)
        .trim_end_matches(".git")
        .to_owned()
}
