use dotenv::dotenv;
use flowsnet_platform_sdk::write_error_log;
use github_flows::{
    get_octo, listen_to_event,
    octocrab::{models::events::payload::EventPayload, models::events::payload::IssuesEventAction, issues::IssueHandler},
};
use openai_flows::{chat_completion, ChatModel, ChatOptions};
use std::env;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() {
    dotenv().ok();

    let login: String = match env::var("login") {
        Err(_) => "alabulei1".to_string(),
        Ok(name) => name,
    };

    let owner: String = match env::var("owner") {
        Err(_) => "robinkits".to_string(),
        Ok(name) => name,
    };

    let repo: String = match env::var("repo") {
        Err(_) => "chat-with-chatgpt".to_string(),
        Ok(name) => name,
    };

    let openai_key_name: String = match env::var("openai_key_name") {
        Err(_) => "chatmichael".to_string(),
        Ok(name) => name,
    };

    listen_to_event(
        &login,
        &owner,
        &repo,
        vec!["issue_comment", "issues"],
        |payload| handler(&login, &owner, &repo, &openai_key_name, payload),
    )
    .await;
}

async fn create_issue_comment (issues: IssueHandler<'_>, issue_number: u64, text: &str) {
    match issues.create_comment(issue_number, text).await {
        Ok(comment) => {
            store_flows::set(
                "last_created_comment",
                serde_json::to_value(comment.id.into_inner()).unwrap(),
           );
        }
        Err(e) => {
            write_error_log!(e.to_string());
        }
    }
}

async fn handler(
    login: &str,
    owner: &str,
    repo: &str,
    openai_key_name: &str,
    payload: EventPayload,
) {
    let octo = get_octo(Some(String::from(login)));
    let issues = octo.issues(owner, repo);

    match payload {
        EventPayload::IssueCommentEvent(e) => {
            let last_comment_id = store_flows::get("last_created_comment").unwrap_or_default();
            if e.comment.id.into_inner() != last_comment_id.as_u64().unwrap_or_default() {
                if let Some(b) = e.comment.body {
                    if let Some(r) = chat_completion(
                        openai_key_name,
                        &format!("issue#{}", e.issue.number),
                        &b,
                        &ChatOptions::default(),
                    ) {
                        if r.restarted {
                            create_issue_comment(issues, e.issue.number, "Sorry, I only have limited amount of time for each conversation. This conversation has expired. If you would like to as a new question, please raise a new Issue. Thanks!").await;
                            return;
                        }
                        create_issue_comment(issues, e.issue.number, &r.choice).await;
                    }
                }
            }
        }

        EventPayload::IssuesEvent(e) => {
            if e.action == IssuesEventAction::Closed {
                return;
            }

            let title = e.issue.title;
            let body = e.issue.body.unwrap_or("".to_string());
            let q = title + "\n" + &body;

            let prompt = "You are a helpful assistant answering questions on GitHub. In your response, you can use simple markdown text to format your answers.\n\nIf someone greets you without asking a question, you should simply respond \"Hello, I am your assistant on GitHub, built by the Second State team. I am ready for your question now!\"";
            let co = ChatOptions {
                model: ChatModel::GPT4,
                restart: true,
                restarted_sentence: Some(prompt),
            };

            if let Some(r) = chat_completion(
                openai_key_name,
                &format!("issue#{}", e.issue.number),
                &q,
                &co,
            ) {
                create_issue_comment(issues, e.issue.number, &r.choice).await;
            }
        }

        _ => (),
    };
}
