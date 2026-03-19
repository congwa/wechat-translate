use crate::config::TranslateConfig as AppTranslateConfig;
use crate::db::{HistorySummaryParticipant, HistorySummarySourceMessage};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

pub const SELF_PARTICIPANT_ID: &str = "__self__";
const MAX_SUMMARY_DAYS: i64 = 14;
const MAX_LINES_PER_DAY: usize = 200;
const MAX_CHARS_PER_DAY: usize = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SummaryLanguage {
    Chinese,
    #[default]
    English,
    Bilingual,
}

impl SummaryLanguage {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "zh" | "chinese" => Ok(Self::Chinese),
            "en" | "english" => Ok(Self::English),
            "bilingual" | "both" => Ok(Self::Bilingual),
            _ => Ok(Self::English),
        }
    }

    fn language_instruction(&self) -> &'static str {
        match self {
            Self::Chinese => "Output in Chinese only.",
            Self::English => "Output in English only.",
            Self::Bilingual => "Output in both English and Chinese. Write the English version first, then the Chinese version below it.",
        }
    }
}

fn chat_daily_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个微信群聊纪要助手。请基于提供的当天聊天记录，用中文输出简洁摘要。

要求：
1. 只输出中文，不要解释你的工作过程
2. 使用以下小标题：
主题：
结论/决定：
待跟进事项：
3. 如果某一项没有明确信息，写"无"
4. 控制在 4-8 行内，信息密度高，不要空话"#.to_string(),
        SummaryLanguage::English => r#"You are a WeChat group chat summary assistant. Based on the provided chat records for the day, output a concise summary in English.

Requirements:
1. Output in English only, do not explain your process
2. Use the following headings:
Topics:
Conclusions/Decisions:
Follow-up Items:
3. If no clear information for a section, write "None"
4. Keep it within 4-8 lines, high information density, no filler"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a WeChat group chat summary assistant. Based on the provided chat records for the day, output a concise summary in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Topics:
Conclusions/Decisions:
Follow-up Items:

[中文]
主题：
结论/决定：
待跟进事项：
3. If no clear information for a section, write "None" / "无"
4. Keep each language version within 4-8 lines"#.to_string(),
    }
}

fn participant_daily_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个群聊成员发言总结助手。请基于提供的当天发言记录，用中文输出简洁摘要。

要求：
1. 只输出中文，不要解释你的工作过程
2. 使用以下小标题：
关注话题：
主要观点/反馈：
承诺/待办：
未解决问题：
3. 如果某一项没有明确信息，写"无"
4. 控制在 4-8 行内，信息密度高，不要空话"#.to_string(),
        SummaryLanguage::English => r#"You are a group chat member summary assistant. Based on the provided speech records for the day, output a concise summary in English.

Requirements:
1. Output in English only, do not explain your process
2. Use the following headings:
Topics of Interest:
Main Opinions/Feedback:
Commitments/TODOs:
Unresolved Issues:
3. If no clear information for a section, write "None"
4. Keep it within 4-8 lines, high information density, no filler"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a group chat member summary assistant. Based on the provided speech records for the day, output a concise summary in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Topics of Interest:
Main Opinions/Feedback:
Commitments/TODOs:
Unresolved Issues:

[中文]
关注话题：
主要观点/反馈：
承诺/待办：
未解决问题：
3. If no clear information for a section, write "None" / "无"
4. Keep each language version within 4-8 lines"#.to_string(),
    }
}

fn chat_overall_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个微信群聊阶段总结助手。请根据多天小结，输出这段时间内群聊的整体总结。

要求：
1. 只输出中文
2. 使用以下小标题：
整体主题：
关键结论/决定：
持续推进事项：
风险或待确认问题：
3. 如果某一项没有明确信息，写"无"
4. 保持简洁，避免重复每天的小结原文"#.to_string(),
        SummaryLanguage::English => r#"You are a WeChat group chat period summary assistant. Based on the daily summaries, output an overall summary for this time period in English.

Requirements:
1. Output in English only
2. Use the following headings:
Overall Topics:
Key Conclusions/Decisions:
Ongoing Items:
Risks or Pending Issues:
3. If no clear information for a section, write "None"
4. Keep it concise, avoid repeating daily summary content"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a WeChat group chat period summary assistant. Based on the daily summaries, output an overall summary in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Overall Topics:
Key Conclusions/Decisions:
Ongoing Items:
Risks or Pending Issues:

[中文]
整体主题：
关键结论/决定：
持续推进事项：
风险或待确认问题：
3. If no clear information for a section, write "None" / "无"
4. Keep it concise"#.to_string(),
    }
}

fn participant_overall_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个群聊成员阶段发言总结助手。请根据多天小结，输出这个人在这段时间内的整体发言画像。

要求：
1. 只输出中文
2. 使用以下小标题：
持续关注话题：
核心观点/反馈：
承诺与行动项：
仍未解决的问题：
3. 如果某一项没有明确信息，写"无"
4. 保持简洁，避免重复每天的小结原文"#.to_string(),
        SummaryLanguage::English => r#"You are a group chat member period summary assistant. Based on the daily summaries, output an overall profile of this person's speech during this period in English.

Requirements:
1. Output in English only
2. Use the following headings:
Ongoing Topics of Interest:
Core Opinions/Feedback:
Commitments & Action Items:
Unresolved Issues:
3. If no clear information for a section, write "None"
4. Keep it concise, avoid repeating daily summary content"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a group chat member period summary assistant. Based on the daily summaries, output an overall profile in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Ongoing Topics of Interest:
Core Opinions/Feedback:
Commitments & Action Items:
Unresolved Issues:

[中文]
持续关注话题：
核心观点/反馈：
承诺与行动项：
仍未解决的问题：
3. If no clear information for a section, write "None" / "无"
4. Keep it concise"#.to_string(),
    }
}

fn global_daily_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个微信消息汇总助手。请基于提供的当天跨群聊消息记录，输出整体动态总结。

要求：
1. 只输出中文，不要解释你的工作过程
2. 使用以下小标题：
群聊活跃度：
主要话题：
值得关注的事项：
3. 按群聊分组简要说明各群动态
4. 控制在 6-10 行内，信息密度高，不要空话"#.to_string(),
        SummaryLanguage::English => r#"You are a WeChat message summary assistant. Based on the provided cross-chat messages for the day, output an overall activity summary in English.

Requirements:
1. Output in English only, do not explain your process
2. Use the following headings:
Chat Activity:
Main Topics:
Notable Items:
3. Group by chat and briefly describe each chat's activity
4. Keep it within 6-10 lines, high information density, no filler"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a WeChat message summary assistant. Based on the provided cross-chat messages for the day, output an overall activity summary in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Chat Activity:
Main Topics:
Notable Items:

[中文]
群聊活跃度：
主要话题：
值得关注的事项：
3. Group by chat and briefly describe each chat's activity
4. Keep each language version within 6-10 lines"#.to_string(),
    }
}

fn global_overall_system_prompt(lang: SummaryLanguage) -> String {
    match lang {
        SummaryLanguage::Chinese => r#"你是一个微信消息阶段汇总助手。请根据多天小结，输出这段时间内的整体动态总结。

要求：
1. 只输出中文
2. 使用以下小标题：
整体活跃趋势：
核心话题汇总：
重要事项与决定：
待跟进问题：
3. 如果某一项没有明确信息，写"无"
4. 保持简洁，避免重复每天的小结原文"#.to_string(),
        SummaryLanguage::English => r#"You are a WeChat message period summary assistant. Based on the daily summaries, output an overall activity summary for this period in English.

Requirements:
1. Output in English only
2. Use the following headings:
Overall Activity Trend:
Core Topics Summary:
Important Items & Decisions:
Follow-up Issues:
3. If no clear information for a section, write "None"
4. Keep it concise, avoid repeating daily summary content"#.to_string(),
        SummaryLanguage::Bilingual => r#"You are a WeChat message period summary assistant. Based on the daily summaries, output an overall activity summary in both English and Chinese.

Requirements:
1. Write the English version first, then the Chinese version
2. Use the following headings:
[English]
Overall Activity Trend:
Core Topics Summary:
Important Items & Decisions:
Follow-up Issues:

[中文]
整体活跃趋势：
核心话题汇总：
重要事项与决定：
待跟进问题：
3. If no clear information for a section, write "None" / "无"
4. Keep it concise"#.to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummaryParticipantRef {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummaryDailyItem {
    pub date: String,
    pub message_count: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummaryResult {
    pub scope: String,
    pub chat_name: String,
    pub participant: Option<HistorySummaryParticipantRef>,
    pub start_date: String,
    pub end_date: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub overall_summary: String,
    pub daily_items: Vec<HistorySummaryDailyItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSummaryResult {
    pub scope: String,
    pub start_date: String,
    pub end_date: String,
    pub message_count: usize,
    pub chat_count: usize,
    pub overall_summary: String,
    pub daily_items: Vec<HistorySummaryDailyItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryScope {
    Chat,
    Participant,
    Global,
}

impl SummaryScope {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "chat" => Ok(Self::Chat),
            "participant" => Ok(Self::Participant),
            "global" => Ok(Self::Global),
            _ => anyhow::bail!("未知总结范围: {}", raw),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Participant => "participant",
            Self::Global => "global",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SummaryRange {
    pub start_date: String,
    pub end_date: String,
}

impl SummaryRange {
    pub fn parse(start_date: &str, end_date: &str) -> Result<Self> {
        let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
            .with_context(|| format!("无效的开始日期: {}", start_date))?;
        let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
            .with_context(|| format!("无效的结束日期: {}", end_date))?;

        if start > end {
            anyhow::bail!("开始日期不能晚于结束日期");
        }

        let days = end.signed_duration_since(start).num_days() + 1;
        if days > MAX_SUMMARY_DAYS {
            anyhow::bail!("自定义日期范围最多支持 {} 天", MAX_SUMMARY_DAYS);
        }

        Ok(Self {
            start_date: start_date.to_string(),
            end_date: end_date.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
struct DailyTranscript {
    date: String,
    message_count: usize,
    transcript: String,
}

pub struct HistorySummaryService {
    client: Client,
    base_url: String,
    api_key: String,
    model_id: String,
    provider_id: String,
}

#[derive(Serialize)]
struct SummaryChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct SummaryChatRequest {
    model: String,
    messages: Vec<SummaryChatMessage>,
    temperature: f64,
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct SummaryChatChoice {
    message: SummaryChatMessageResponse,
}

#[derive(Deserialize)]
struct SummaryChatMessageResponse {
    content: String,
}

#[derive(Deserialize)]
struct SummaryChatResponse {
    choices: Vec<SummaryChatChoice>,
}

impl HistorySummaryService {
    pub fn from_translate_config(config: &AppTranslateConfig) -> Result<Self> {
        if config.provider != "ai" {
            anyhow::bail!("总结功能需要在设置页启用 AI 翻译配置");
        }

        if config.ai_api_key.trim().is_empty() || config.ai_model_id.trim().is_empty() {
            anyhow::bail!("总结功能需要在设置页完成 AI 模型与 API Key 配置");
        }

        let provider_id = config.ai_provider_id.trim().to_string();
        let base_url = resolve_base_url(&provider_id, config.ai_base_url.trim())?;
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(config.timeout_seconds.max(5.0)))
            .build()
            .context("create summary http client failed")?;

        Ok(Self {
            client,
            base_url,
            api_key: config.ai_api_key.clone(),
            model_id: config.ai_model_id.clone(),
            provider_id,
        })
    }

    pub async fn summarize(
        &self,
        scope: SummaryScope,
        chat_name: &str,
        participant: Option<&HistorySummaryParticipant>,
        participant_count: usize,
        start_date: &str,
        end_date: &str,
        messages: &[HistorySummarySourceMessage],
        language: SummaryLanguage,
    ) -> Result<HistorySummaryResult> {
        let daily_transcripts = build_daily_transcripts(messages);
        let message_count = daily_transcripts
            .iter()
            .map(|item| item.message_count)
            .sum();

        if daily_transcripts.is_empty() {
            return Ok(HistorySummaryResult {
                scope: scope.as_str().to_string(),
                chat_name: chat_name.to_string(),
                participant: participant.map(|item| HistorySummaryParticipantRef {
                    id: item.id.clone(),
                    label: item.label.clone(),
                }),
                start_date: start_date.to_string(),
                end_date: end_date.to_string(),
                message_count: 0,
                participant_count,
                overall_summary: String::new(),
                daily_items: Vec::new(),
            });
        }

        let mut daily_items = Vec::with_capacity(daily_transcripts.len());
        for item in &daily_transcripts {
            let summary = self
                .summarize_daily(scope.clone(), chat_name, participant, item, language)
                .await?;
            daily_items.push(HistorySummaryDailyItem {
                date: item.date.clone(),
                message_count: item.message_count,
                summary,
            });
        }

        let overall_summary = if daily_items.len() == 1 {
            daily_items[0].summary.clone()
        } else {
            self.summarize_overall(scope.clone(), chat_name, participant, &daily_items, language)
                .await?
        };

        Ok(HistorySummaryResult {
            scope: scope.as_str().to_string(),
            chat_name: chat_name.to_string(),
            participant: participant.map(|item| HistorySummaryParticipantRef {
                id: item.id.clone(),
                label: item.label.clone(),
            }),
            start_date: start_date.to_string(),
            end_date: end_date.to_string(),
            message_count,
            participant_count,
            overall_summary,
            daily_items,
        })
    }

    async fn summarize_daily(
        &self,
        scope: SummaryScope,
        chat_name: &str,
        participant: Option<&HistorySummaryParticipant>,
        transcript: &DailyTranscript,
        language: SummaryLanguage,
    ) -> Result<String> {
        let participant_label = participant
            .map(|item| item.label.as_str())
            .unwrap_or("全部成员");
        let user_prompt = match scope {
            SummaryScope::Chat => format!(
                "群聊：{}\n日期：{}\n消息数：{}\n\n以下是当天聊天记录：\n{}\n",
                chat_name, transcript.date, transcript.message_count, transcript.transcript
            ),
            SummaryScope::Participant => format!(
                "群聊：{}\n成员：{}\n日期：{}\n消息数：{}\n\n以下是该成员当天发言记录：\n{}\n",
                chat_name,
                participant_label,
                transcript.date,
                transcript.message_count,
                transcript.transcript
            ),
            SummaryScope::Global => unreachable!("Global scope uses summarize_global_daily"),
        };

        let system_prompt = match scope {
            SummaryScope::Chat => chat_daily_system_prompt(language),
            SummaryScope::Participant => participant_daily_system_prompt(language),
            SummaryScope::Global => unreachable!("Global scope uses summarize_global_daily"),
        };

        self.complete(&system_prompt, &user_prompt, Some(1200)).await
    }

    async fn summarize_overall(
        &self,
        scope: SummaryScope,
        chat_name: &str,
        participant: Option<&HistorySummaryParticipant>,
        daily_items: &[HistorySummaryDailyItem],
        language: SummaryLanguage,
    ) -> Result<String> {
        let participant_label = participant
            .map(|item| item.label.as_str())
            .unwrap_or("全部成员");
        let daily_text = daily_items
            .iter()
            .map(|item| {
                format!(
                    "{}（{} 条消息）\n{}",
                    item.date, item.message_count, item.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_prompt = match scope {
            SummaryScope::Chat => format!(
                "群聊：{}\n以下是按天整理的小结，请输出整个时间范围内的整体总结：\n\n{}",
                chat_name, daily_text
            ),
            SummaryScope::Participant => format!(
                "群聊：{}\n成员：{}\n以下是按天整理的小结，请输出整个时间范围内的整体总结：\n\n{}",
                chat_name, participant_label, daily_text
            ),
            SummaryScope::Global => unreachable!("Global scope uses summarize_global_overall"),
        };

        let system_prompt = match scope {
            SummaryScope::Chat => chat_overall_system_prompt(language),
            SummaryScope::Participant => participant_overall_system_prompt(language),
            SummaryScope::Global => unreachable!("Global scope uses summarize_global_overall"),
        };

        self.complete(&system_prompt, &user_prompt, Some(1200)).await
    }

    /// 生成跨所有群聊的全局总结
    pub async fn summarize_global(
        &self,
        chat_count: usize,
        start_date: &str,
        end_date: &str,
        messages: &[HistorySummarySourceMessage],
        language: SummaryLanguage,
    ) -> Result<GlobalSummaryResult> {
        let daily_transcripts = build_global_daily_transcripts(messages);
        let message_count = daily_transcripts
            .iter()
            .map(|item| item.message_count)
            .sum();

        if daily_transcripts.is_empty() {
            return Ok(GlobalSummaryResult {
                scope: SummaryScope::Global.as_str().to_string(),
                start_date: start_date.to_string(),
                end_date: end_date.to_string(),
                message_count: 0,
                chat_count,
                overall_summary: String::new(),
                daily_items: Vec::new(),
            });
        }

        let mut daily_items = Vec::with_capacity(daily_transcripts.len());
        for item in &daily_transcripts {
            let summary = self.summarize_global_daily(item, language).await?;
            daily_items.push(HistorySummaryDailyItem {
                date: item.date.clone(),
                message_count: item.message_count,
                summary,
            });
        }

        let overall_summary = if daily_items.len() == 1 {
            daily_items[0].summary.clone()
        } else {
            self.summarize_global_overall(&daily_items, language).await?
        };

        Ok(GlobalSummaryResult {
            scope: SummaryScope::Global.as_str().to_string(),
            start_date: start_date.to_string(),
            end_date: end_date.to_string(),
            message_count,
            chat_count,
            overall_summary,
            daily_items,
        })
    }

    async fn summarize_global_daily(&self, transcript: &DailyTranscript, language: SummaryLanguage) -> Result<String> {
        let user_prompt = format!(
            "日期：{}\n消息数：{}\n\n以下是当天跨群聊消息记录：\n{}\n",
            transcript.date, transcript.message_count, transcript.transcript
        );
        let system_prompt = global_daily_system_prompt(language);
        self.complete(&system_prompt, &user_prompt, Some(1200))
            .await
    }

    async fn summarize_global_overall(
        &self,
        daily_items: &[HistorySummaryDailyItem],
        language: SummaryLanguage,
    ) -> Result<String> {
        let daily_text = daily_items
            .iter()
            .map(|item| {
                format!(
                    "{}（{} 条消息）\n{}",
                    item.date, item.message_count, item.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_prompt = format!(
            "以下是按天整理的小结，请输出整个时间范围内的整体总结：\n\n{}",
            daily_text
        );
        let system_prompt = global_overall_system_prompt(language);
        self.complete(&system_prompt, &user_prompt, Some(1200))
            .await
    }

    async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: Option<u32>,
    ) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = SummaryChatRequest {
            model: self.model_id.clone(),
            messages: vec![
                SummaryChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                SummaryChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            temperature: 0.3,
            max_tokens,
        };

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key));

        if self.provider_id == "anthropic" {
            req = req
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01");
        }

        let resp = req
            .json(&request)
            .send()
            .await
            .context("summary AI request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(300).collect();
            anyhow::bail!("总结服务 HTTP {}: {}", status.as_u16(), snippet);
        }

        let response: SummaryChatResponse = resp
            .json()
            .await
            .context("summary AI response parse failed")?;

        response
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| anyhow::anyhow!("总结服务返回空结果"))
    }
}

fn resolve_base_url(provider_id: &str, custom_base_url: &str) -> Result<String> {
    if !custom_base_url.is_empty() {
        return Ok(custom_base_url.trim_end_matches('/').to_string());
    }

    match provider_id {
        "openai" => Ok("https://api.openai.com/v1".to_string()),
        "anthropic" => Ok("https://api.anthropic.com/v1".to_string()),
        "deepseek" => Ok("https://api.deepseek.com".to_string()),
        "groq" => Ok("https://api.groq.com/openai/v1".to_string()),
        "mistral" => Ok("https://api.mistral.ai/v1".to_string()),
        "openrouter" => Ok("https://openrouter.ai/api/v1".to_string()),
        "together" => Ok("https://api.together.xyz/v1".to_string()),
        "perplexity" => Ok("https://api.perplexity.ai".to_string()),
        "moonshot" => Ok("https://api.moonshot.cn/v1".to_string()),
        _ => anyhow::bail!("总结功能缺少可用的 AI base_url"),
    }
}

fn build_daily_transcripts(messages: &[HistorySummarySourceMessage]) -> Vec<DailyTranscript> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for message in messages {
        let Some((date, line)) = format_summary_line(message) else {
            continue;
        };
        grouped.entry(date.clone()).or_default().push(line);
        *counts.entry(date).or_default() += 1;
    }

    grouped
        .into_iter()
        .map(|(date, lines)| {
            let message_count = counts.get(&date).copied().unwrap_or(0);
            let transcript = trim_transcript_lines(&lines);
            DailyTranscript {
                date,
                message_count,
                transcript,
            }
        })
        .collect()
}

/// 构建全局总结的每日 transcript，包含群聊名称
fn build_global_daily_transcripts(messages: &[HistorySummarySourceMessage]) -> Vec<DailyTranscript> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for message in messages {
        let Some((date, line)) = format_global_summary_line(message) else {
            continue;
        };
        grouped.entry(date.clone()).or_default().push(line);
        *counts.entry(date).or_default() += 1;
    }

    grouped
        .into_iter()
        .map(|(date, lines)| {
            let message_count = counts.get(&date).copied().unwrap_or(0);
            let transcript = trim_transcript_lines(&lines);
            DailyTranscript {
                date,
                message_count,
                transcript,
            }
        })
        .collect()
}

/// 格式化全局总结的单行消息，包含群聊名称
fn format_global_summary_line(message: &HistorySummarySourceMessage) -> Option<(String, String)> {
    let content = if message.image_path.is_some() {
        "[图片]".to_string()
    } else {
        message.content.trim().to_string()
    };

    if content.is_empty() {
        return None;
    }

    let detected_at = message.detected_at.trim();
    let date = detected_at.get(0..10)?.to_string();
    let time = detected_at.get(11..16).unwrap_or("00:00");
    let chat_name = message.chat_name.trim();
    let sender = if message.is_self {
        "我".to_string()
    } else if !message.sender.trim().is_empty() {
        message.sender.trim().to_string()
    } else {
        chat_name.to_string()
    };

    Some((date, format!("{time} | [{chat_name}] {sender} | {content}")))
}

fn trim_transcript_lines(lines: &[String]) -> String {
    let mut selected = if lines.len() > MAX_LINES_PER_DAY {
        lines[lines.len() - MAX_LINES_PER_DAY..].to_vec()
    } else {
        lines.to_vec()
    };

    let mut removed = lines.len().saturating_sub(selected.len());
    while selected.join("\n").chars().count() > MAX_CHARS_PER_DAY && !selected.is_empty() {
        selected.remove(0);
        removed += 1;
    }

    let mut output = selected.join("\n");
    if removed > 0 {
        output = format!("[已省略更早的 {} 条消息]\n{}", removed, output);
    }
    output
}

fn format_summary_line(message: &HistorySummarySourceMessage) -> Option<(String, String)> {
    let content = if message.image_path.is_some() {
        "[图片]".to_string()
    } else {
        message.content.trim().to_string()
    };

    if content.is_empty() {
        return None;
    }

    let detected_at = message.detected_at.trim();
    let date = detected_at.get(0..10)?.to_string();
    let time = detected_at.get(11..16).unwrap_or("00:00");
    let sender = if message.is_self {
        "我".to_string()
    } else if !message.sender.trim().is_empty() {
        message.sender.trim().to_string()
    } else {
        message.chat_name.trim().to_string()
    };

    Some((date, format!("{time} | {sender} | {content}")))
}

#[cfg(test)]
mod tests {
    use super::{
        build_daily_transcripts, format_summary_line, HistorySummarySourceMessage, SummaryRange,
        SummaryScope,
    };

    fn sample_message(
        detected_at: &str,
        sender: &str,
        content: &str,
        is_self: bool,
    ) -> HistorySummarySourceMessage {
        HistorySummarySourceMessage {
            chat_name: "项目群".to_string(),
            sender: sender.to_string(),
            content: content.to_string(),
            is_self,
            detected_at: detected_at.to_string(),
            image_path: None,
        }
    }

    #[test]
    fn summary_scope_should_parse_known_values() {
        assert_eq!(SummaryScope::parse("chat").unwrap(), SummaryScope::Chat);
        assert_eq!(
            SummaryScope::parse("participant").unwrap(),
            SummaryScope::Participant
        );
        assert!(SummaryScope::parse("unknown").is_err());
    }

    #[test]
    fn summary_range_should_validate_order_and_limit() {
        assert!(SummaryRange::parse("2026-03-01", "2026-03-14").is_ok());
        assert!(SummaryRange::parse("2026-03-02", "2026-03-01").is_err());
        assert!(SummaryRange::parse("2026-03-01", "2026-03-15").is_err());
    }

    #[test]
    fn format_summary_line_should_use_me_for_self_messages() {
        let line =
            format_summary_line(&sample_message("2026-03-18 09:12:00", "", "我来跟进", true))
                .expect("format line");

        assert_eq!(line.0, "2026-03-18");
        assert_eq!(line.1, "09:12 | 我 | 我来跟进");
    }

    #[test]
    fn build_daily_transcripts_should_group_by_day_and_skip_empty_messages() {
        let messages = vec![
            sample_message("2026-03-18 09:12:00", "张三", "同步下进度", false),
            sample_message("2026-03-18 10:30:00", "", "   ", true),
            sample_message("2026-03-19 11:00:00", "李四", "收到", false),
        ];

        let daily = build_daily_transcripts(&messages);
        assert_eq!(daily.len(), 2);
        assert_eq!(daily[0].date, "2026-03-18");
        assert_eq!(daily[0].message_count, 1);
        assert!(daily[0].transcript.contains("09:12 | 张三 | 同步下进度"));
        assert_eq!(daily[1].date, "2026-03-19");
        assert_eq!(daily[1].message_count, 1);
    }
}
