//! Performing evaluations: scraping the correction grid from
//! `/scale_teams/{id}/edit` and submitting it back as the Rails form.

use reqwest::header::{HeaderMap, HeaderValue};

use super::models::{EvalForm, EvalSection, EvalSubmission};
use super::{Api, ApiError, ApiResult, PROJECTS_BASE};

impl Api {
    /// The evaluation form for one upcoming duty. The page is plain
    /// cookie-authenticated HTML; a dead session bounces to signin.
    pub async fn evaluation_form(&self, scale_team_id: u64) -> ApiResult<EvalForm> {
        let send = || async {
            let url = format!("{PROJECTS_BASE}/scale_teams/{scale_team_id}/edit");
            let resp = self.http.get(&url).send().await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("evaluation form", resp).await);
            }
            if resp
                .url()
                .host_str()
                .is_some_and(|host| host.contains("signin"))
            {
                return Err(ApiError::SessionExpired);
            }
            let html = resp
                .text()
                .await
                .map_err(|error| ApiError::Network(error.to_string()))?;
            parse_eval_form(&html)
        };
        match send().await {
            Ok(form) => Ok(form),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    send().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Submit a finished evaluation. Success is the redirect target of
    /// the 302 — the corrected's project page; a redirect back to the
    /// form means the server refused (already submitted or invalid).
    pub async fn submit_evaluation(&self, submission: &EvalSubmission) -> ApiResult<String> {
        // The page's form action is a relative path ("/projects/…") — the
        // browser resolves it against the host; reqwest needs it anchored.
        let url = if submission.action.starts_with('/') {
            format!("{PROJECTS_BASE}{}", submission.action)
        } else {
            submission.action.clone()
        };
        let send = || async {
            let mut headers = HeaderMap::new();
            headers.insert("Origin", HeaderValue::from_str(PROJECTS_BASE).unwrap());
            headers.insert(
                "Referer",
                HeaderValue::from_str(&format!(
                    "{PROJECTS_BASE}/scale_teams/{}/edit",
                    submission.scale_team_id
                ))
                .unwrap(),
            );
            let body = submission_body(submission);
            let resp = self
                .noredirect
                .post(&url)
                .headers(headers)
                .form(&body[..])
                .send()
                .await?;
            let status = resp.status();
            if let Some(location) = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
            {
                if location.contains("signin") {
                    return Err(ApiError::SessionExpired);
                }
                if status.as_u16() == 302 || status.as_u16() == 303 {
                    if location.contains("/scale_teams/") {
                        return Err(ApiError::Other(
                            "rejected — evaluation already submitted or incomplete".into(),
                        ));
                    }
                    return Ok(location.to_owned());
                }
            }
            // No redirect: Rails re-rendered the form with validation
            // errors (unanswered questions, empty comment).
            Err(ApiError::from_response("evaluation submit", resp).await)
        };
        match send().await {
            Ok(target) => Ok(target),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    send().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }
}

/// The exact field set the browser posts, in order — including Rails'
/// duplicate `scale_team[flag_id]` pair (hidden fallback first, selected
/// radio second) and the `commit` button that marks a real finish.
pub fn submission_body(submission: &EvalSubmission) -> Vec<(String, String)> {
    let mut body: Vec<(String, String)> = vec![
        ("utf8".into(), "✓".into()),
        ("_method".into(), "patch".into()),
        (
            "authenticity_token".into(),
            submission.authenticity_token.clone(),
        ),
    ];
    for (index, answer) in submission.answers.iter().enumerate() {
        let prefix = format!("[scale_team][answers_attributes][{index}]");
        body.push((
            format!("{prefix}[id]"),
            answer.answer_id.clone().unwrap_or_default(),
        ));
        body.push((
            format!("{prefix}[question_id]"),
            answer.question_id.to_string(),
        ));
        body.push((format!("{prefix}[value]"), answer.value.clone()));
    }
    body.push(("scale_team[flag_id]".into(), String::new()));
    body.push(("scale_team[flag_id]".into(), submission.flag_id.to_string()));
    body.push(("scale_team[comment]".into(), submission.comment.clone()));
    body.push(("token".into(), String::new()));
    body.push(("commit".into(), "Finish evaluation".into()));
    body
}

// ------------------------------------------------------------- parsing ----

/// Scrape the edit page into an [`EvalForm`]. Every selector mirrors the
/// live DOM (fixture: `fixtures/eval_form.html`, captured 2026-08-26).
pub fn parse_eval_form(html: &str) -> ApiResult<EvalForm> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let bad = |detail: &str| ApiError::Parse {
        endpoint: "evaluation form",
        detail: detail.into(),
    };

    let form_sel = Selector::parse("form.simple_form").unwrap();
    let form = document
        .select(&form_sel)
        .next()
        .ok_or_else(|| bad("no form"))?;
    let action = form
        .value()
        .attr("action")
        .ok_or_else(|| bad("no action"))?
        .to_owned();

    let token_sel = Selector::parse("input[name='authenticity_token']").unwrap();
    let authenticity_token = document
        .select(&token_sel)
        .find_map(|input| input.value().attr("value"))
        .unwrap_or_default()
        .to_owned();

    let one_text = |selector: &str| -> Option<String> {
        Selector::parse(selector)
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
    };

    let project = one_text(".scale-section-infos h3 a").unwrap_or_default();
    let repo = document
        .select(&Selector::parse("input.repo-url").unwrap())
        .next()
        .and_then(|input| input.value().attr("value"))
        .map(str::to_owned);

    let mut correcteds = Vec::new();
    let user_sel = Selector::parse(".team-user-item .bg-image-item").unwrap();
    for user in document.select(&user_sel) {
        if let Some(login) = user.value().attr("title") {
            correcteds.push(login.to_owned());
        }
    }

    // Sections walk their direct children: an h4 opens a question, the
    // elements after it are its guidelines, and the answers div closes
    // it. (Selecting the guidelines <p> directly is not enough — its
    // block-shaped children auto-close it, so html5ever hands the prose
    // to the section instead.)
    let mut sections = Vec::new();
    let section_sel = Selector::parse("section.scale-section-item").unwrap();
    let title_sel = Selector::parse(".section-header h3").unwrap();
    for section in document.select(&section_sel) {
        let title = section
            .select(&title_sel)
            .next()
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let mut questions = Vec::new();
        let mut name: Option<String> = None;
        let mut guidelines: Vec<String> = Vec::new();
        for child in section.child_elements() {
            let tag = child.value().name();
            let classes = child.value().attr("class").unwrap_or_default();
            if tag == "h4" && classes.contains("scale-question-name") {
                name = Some(
                    child
                        .text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                guidelines = Vec::new();
            } else if tag == "div" && classes.contains("scale-question-answers") {
                if let Some(name) = name.take()
                    // Paragraphs and lists arrive as separate children —
                    // a blank line between them mirrors the page.
                    && let Some(question) =
                        parse_question(name, guidelines.join("\n\n"), &child)
                {
                    questions.push(question);
                }
                guidelines = Vec::new();
            } else if name.is_some() {
                let text = html_to_text(&child.inner_html());
                if !text.is_empty() {
                    guidelines.push(text);
                }
            }
        }
        sections.push(EvalSection { title, questions });
    }

    // Flags: the hidden radio block holds the actual inputs; the visible
    // ratings row decides which of them are positive.
    let mut flag_positive: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    let ratings_sel = Selector::parse("div[id^='ratings-'] label").unwrap();
    let for_sel_prefix = "scale_team_flag_id_";
    for label in document.select(&ratings_sel) {
        let classes = label.value().attr("class").unwrap_or_default();
        if let Some(id) = label
            .value()
            .attr("for")
            .and_then(|f| f.strip_prefix(for_sel_prefix))
            .and_then(|id| id.parse::<u32>().ok())
        {
            flag_positive.insert(id, classes.contains("positive"));
        }
    }
    let mut flags = Vec::new();
    let mut default_flag = None;
    // Each radio sits inside its own label, which carries the name.
    let flag_label_sel = Selector::parse(".scale-final-evaluation label").unwrap();
    let radio_sel = Selector::parse("input[type='radio']").unwrap();
    for label in document.select(&flag_label_sel) {
        let Some(input) = label.select(&radio_sel).next() else {
            continue;
        };
        let Some(id) = input
            .value()
            .attr("value")
            .and_then(|v| v.parse::<u32>().ok())
        else {
            continue;
        };
        if input.value().attr("checked").is_some() {
            default_flag = Some(id);
        }
        let name = label
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        flags.push(super::models::EvalFlag {
            id,
            name,
            positive: *flag_positive.get(&id).unwrap_or(&false),
        });
    }

    if sections.is_empty() || flags.is_empty() {
        return Err(bad("no sections or flags — not an evaluation page"));
    }

    // Positives (Ok, Outstanding) lead the list — the ratings pane splits
    // them from the immediate-0 flags below. Stable, so page order holds
    // within each group.
    flags.sort_by_key(|flag| !flag.positive);

    Ok(EvalForm {
        action,
        authenticity_token,
        project,
        correcteds,
        repo,
        sections,
        flags,
        default_flag,
    })
}

/// One `.scale-question-answers` block → question id, existing answer id
/// and the input control (boolean radios, multi-choice radios, or a
/// range slider with its skill grouping).
fn parse_question(
    name: String,
    guidelines: String,
    block: &scraper::ElementRef,
) -> Option<super::models::EvalQuestion> {
    use scraper::Selector;

    let by_name_suffix =
        |suffix: &str| Selector::parse(&format!("input[name$='{suffix}']")).unwrap();
    let question_id = block
        .select(&by_name_suffix("[question_id]"))
        .find_map(|input| input.value().attr("value"))
        .and_then(|value| value.parse::<u64>().ok())?;
    let answer_id = block
        .select(&by_name_suffix("[id]"))
        .find_map(|input| input.value().attr("value"))
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    let value_sel = by_name_suffix("[value]");
    let values: Vec<scraper::ElementRef> = block.select(&value_sel).collect();

    let kind = if let Some(range) = values
        .iter()
        .find(|input| input.value().attr("type") == Some("range"))
    {
        let attrs = range.value();
        let kind = attrs.attr("data-kind").map(str::to_owned);
        let default_max: i64 = if kind.as_deref() == Some("bonus") {
            25
        } else {
            100
        };
        super::models::QuestionKind::Range {
            min: attrs.attr("min").and_then(|v| v.parse().ok()).unwrap_or(0),
            max: attrs
                .attr("max")
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_max),
            step: attrs
                .attr("step")
                .and_then(|v| v.parse().ok())
                .filter(|s| *s > 0)
                .unwrap_or(1),
            value: attrs
                .attr("value")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            skill: attrs.attr("data-slider-id").map(str::to_owned),
            kind,
        }
    } else if !values.is_empty() {
        // Radio group: a plain 1/0 pair is a boolean; anything else is a
        // multi-option choice.
        let mut options = Vec::new();
        for input in &values {
            let value = input.value().attr("value").unwrap_or_default().to_owned();
            let selected = input.value().attr("checked").is_some();
            let label = input
                .value()
                .attr("id")
                .and_then(|id| Selector::parse(&format!("label[for='{id}']")).ok())
                .and_then(|sel| block.select(&sel).next())
                .map(|label| {
                    label
                        .text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| match value.as_str() {
                    "1" => "Yes".into(),
                    "0" => "No".into(),
                    _ => value.clone(),
                });
            options.push(super::models::EvalChoice {
                value,
                label,
                selected,
            });
        }
        let boolean_pair = options.len() == 2
            && options.iter().any(|o| o.value == "1")
            && options.iter().any(|o| o.value == "0");
        if boolean_pair {
            super::models::QuestionKind::Boolean {
                yes: options.iter().any(|o| o.selected && o.value == "1"),
            }
        } else {
            super::models::QuestionKind::Choice { options }
        }
    } else {
        // No input control found — treat as a yes/no so the flow works.
        super::models::QuestionKind::Boolean { yes: false }
    };

    Some(super::models::EvalQuestion {
        question_id,
        name,
        guidelines,
        answer_id,
        kind,
    })
}

/// Minimal HTML → readable text that mirrors the page's structure:
/// `<li>` items become `- ` bullets, `<code>` fragments are backticked,
/// `<p>`/`<ul>` separate paragraphs (rendered as a blank line) and `<br>`
/// breaks a line. Whitespace inside text runs collapses, so the page's
/// source formatting never breaks a sentence mid-way.
pub fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                let closing = chars.peek() == Some(&'/');
                let mut tag = String::new();
                for tag_ch in chars.by_ref() {
                    if tag_ch == '>' {
                        break;
                    }
                    if tag_ch.is_whitespace() && !tag.is_empty() {
                        // Attributes: skip past the rest of the tag.
                        for skipped in chars.by_ref() {
                            if skipped == '>' {
                                break;
                            }
                        }
                        break;
                    }
                    if !tag_ch.is_whitespace() {
                        tag.push(tag_ch.to_ascii_lowercase());
                    }
                }
                match (tag.trim_matches('/'), closing) {
                    ("li", false) => text.push_str("\n- "),
                    ("p" | "ul" | "ol", false) => text.push_str("\n\n"),
                    ("br", _) => text.push('\n'),
                    ("code", _) => text.push('`'),
                    _ => {}
                }
            }
            '&' => {
                let mut entity = String::new();
                let mut stream = chars.clone();
                while let Some(&entity_ch) = stream.peek() {
                    entity.push(entity_ch);
                    stream.next();
                    if entity.len() > 6 {
                        break;
                    }
                    if entity_ch == ';' {
                        break;
                    }
                }
                let decoded = match entity.as_str() {
                    "#39;" | "apos;" => Some('\''),
                    "quot;" => Some('"'),
                    "amp;" => Some('&'),
                    "lt;" => Some('<'),
                    "gt;" => Some('>'),
                    "nbsp;" => Some(' '),
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    for _ in entity.chars() {
                        chars.next();
                    }
                    text.push(decoded);
                } else {
                    text.push('&');
                }
            }
            other if other.is_whitespace() => {
                // Source formatting collapses to one space; the line
                // structure comes from the tags alone.
                if !text.ends_with(' ') && !text.ends_with('\n') {
                    text.push(' ');
                }
            }
            other => text.push(other),
        }
    }
    // Trim the lines and collapse the blanks the block tags left into a
    // single blank line between paragraphs.
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if lines.last().is_some_and(|last| !last.is_empty()) {
                lines.push("");
            }
        } else {
            lines.push(line);
        }
    }
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{EvalAnswer, QuestionKind};

    const FIXTURE: &str = include_str!("fixtures/eval_form.html");

    #[test]
    fn parses_the_captured_python_module_form() {
        let form = parse_eval_form(FIXTURE).expect("form parses");
        assert_eq!(
            form.action,
            "/projects/python-module-01/project_sessions/15250/evaluations/12767/scales/65265/scale_teams/9720568"
        );
        assert!(!form.authenticity_token.is_empty());
        assert_eq!(form.project, "Python Module 01");
        assert_eq!(form.correcteds, vec!["clferrei"]);
        assert!(
            form.repo
                .as_deref()
                .unwrap_or_default()
                .contains("vogsphere")
        );

        assert_eq!(form.sections.len(), 2);
        assert_eq!(form.sections[0].title, "Preliminaries");
        assert_eq!(form.sections[1].title, "Exercises");
        let questions = form.questions();
        assert_eq!(questions.len(), 9);
        assert_eq!(questions[0].name, "Basics");
        assert_eq!(questions[0].question_id, 62_7776);
        assert_eq!(questions[8].question_id, 62_7795);
        // Every control on this scale is a yes/no defaulting to No.
        assert!(
            questions
                .iter()
                .all(|q| q.kind == QuestionKind::Boolean { yes: false })
        );
        // Guidelines survive as readable text with the entity decoded.
        assert!(questions[0].guidelines.contains("ft_garden_intro.py"));
        assert!(questions[0].guidelines.contains("learner's"));

        // Flags: two positive, ten negative, Ok preselected.
        assert_eq!(form.flags.len(), 12);
        // Positives lead the list, negatives trail it (the ratings pane
        // draws its separator on this split).
        assert!(form.flags.iter().take(2).all(|flag| flag.positive));
        assert!(form.flags.iter().skip(2).all(|flag| !flag.positive));
        let ok = form.flags.iter().find(|f| f.name == "Ok").unwrap();
        assert!(ok.positive && ok.id == 1);
        let outstanding = form.flags.iter().find(|f| f.id == 9).unwrap();
        assert_eq!(outstanding.name, "Outstanding project");
        assert!(outstanding.positive);
        let cheat = form.flags.iter().find(|f| f.id == 7).unwrap();
        assert_eq!(cheat.name, "Cheat");
        assert!(!cheat.positive);
        let crash = form.flags.iter().find(|f| f.name == "Crash").unwrap();
        assert_eq!(crash.id, 8);
        assert_eq!(form.default_flag, Some(1));
    }

    #[test]
    fn parses_slider_questions_with_skill_groups() {
        // Synthetic scale in the shape the Scales module documents:
        // range inputs grouped per skill with standard/bonus kinds.
        let html = r#"
        <form class="simple_form" action="/projects/ft_ls/project_sessions/1/evaluations/2/scales/3/scale_teams/4">
        <input type="hidden" name="authenticity_token" value="tok" />
        <section class='scale-section-item'>
          <div class='section-header'><h3>Grading</h3></div>
          <h4 class='scale-question-name'>Rigor</h4>
          <p class='scale-section-guidelines'>Use the sliders.</p>
          <div class='scale-question-answers'>
            <input name="[scale_team][answers_attributes][0][question_id]" type="hidden" value="111" />
            <input name="[scale_team][answers_attributes][0][id]" type="hidden" value="55" />
            <input data-slider-id="7" data-kind="standard" type="range" min="0" max="100" step="5" value="30"
                   name="[scale_team][answers_attributes][0][value]" />
          </div>
          <h4 class='scale-question-name'>Bonus</h4>
          <p class='scale-section-guidelines'>Bonus part.</p>
          <div class='scale-question-answers'>
            <input name="[scale_team][answers_attributes][1][question_id]" type="hidden" value="112" />
            <input data-slider-id="7" data-kind="bonus" type="range" min="0" max="25" step="1" value="5"
                   name="[scale_team][answers_attributes][1][value]" />
          </div>
        </section>
        </form>
        <div class='scale-final-evaluation'><label><input type="radio" value="1" checked="checked" name="scale_team[flag_id]" />Ok</label></div>
        "#;
        let form = parse_eval_form(html).expect("form parses");
        let questions = form.questions();
        assert_eq!(questions.len(), 2);
        assert_eq!(
            questions[0].kind,
            QuestionKind::Range {
                min: 0,
                max: 100,
                step: 5,
                value: 30,
                skill: Some("7".into()),
                kind: Some("standard".into()),
            }
        );
        assert_eq!(questions[0].answer_id.as_deref(), Some("55"));
        assert!(matches!(
            &questions[1].kind,
            QuestionKind::Range { max, kind, .. } if *max == 25 && kind.as_deref() == Some("bonus")
        ));
    }

    #[test]
    fn submission_body_matches_the_captured_post() {
        let submission = EvalSubmission {
            scale_team_id: 9_720_568,
            action: "/projects/x/scale_teams/9720568".into(),
            authenticity_token: "tok".into(),
            answers: vec![
                EvalAnswer {
                    question_id: 62_7776,
                    value: "1".into(),
                    answer_id: None,
                },
                EvalAnswer {
                    question_id: 62_7778,
                    value: "0".into(),
                    answer_id: Some("99".into()),
                },
            ],
            flag_id: 9,
            comment: "Super gedaan!\nVeel success".into(),
        };
        let body = submission_body(&submission);
        let pairs: Vec<(&str, &str)> = body.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            pairs,
            vec![
                ("utf8", "✓"),
                ("_method", "patch"),
                ("authenticity_token", "tok"),
                ("[scale_team][answers_attributes][0][id]", ""),
                ("[scale_team][answers_attributes][0][question_id]", "627776"),
                ("[scale_team][answers_attributes][0][value]", "1"),
                ("[scale_team][answers_attributes][1][id]", "99"),
                ("[scale_team][answers_attributes][1][question_id]", "627778"),
                ("[scale_team][answers_attributes][1][value]", "0"),
                // Rails' hidden fallback first, selected radio second.
                ("scale_team[flag_id]", ""),
                ("scale_team[flag_id]", "9"),
                ("scale_team[comment]", "Super gedaan!\nVeel success"),
                ("token", ""),
                ("commit", "Finish evaluation"),
            ]
        );
    }

    #[test]
    fn html_to_text_keeps_structure_and_decodes_entities() {
        assert_eq!(
            html_to_text("<p>a &amp; b</p><ul><li>one</li><li>two</li></ul>"),
            "a & b\n\n- one\n- two"
        );
        assert_eq!(html_to_text("line<br/>next"), "line\nnext");
        // `code` fragments backtick; entities decode inside them.
        assert_eq!(
            html_to_text("<p>run <code>cd x &amp;&amp; uv sync</code> next</p>"),
            "run `cd x && uv sync` next"
        );
        // Source newlines inside an item collapse to spaces.
        assert_eq!(
            html_to_text("<li>root,\n   i.e. <code>x</code> y</li>"),
            "- root, i.e. `x` y"
        );
    }
}
