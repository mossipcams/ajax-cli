use agent_client_protocol::schema::{v1, v2, MaybeUndefined};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpViewStatus {
    Ready,
    Running,
    Waiting,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEntry {
    pub id: String,
    pub title: String,
    pub status: ToolStatus,
    pub content: Option<String>,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionEntry {
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptEntry {
    User(String),
    AgentText(String),
    Tool(ToolEntry),
    Permission(PermissionEntry),
    TerminalRaw(String),
    Plain(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpViewState {
    pub provider: String,
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub status: AcpViewStatus,
    pub transcript: Vec<TranscriptEntry>,
    pub prompt_draft: String,
    pub selected_tool_index: Option<usize>,
}

impl AcpViewState {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: None,
            session_id: None,
            status: AcpViewStatus::Ready,
            transcript: Vec::new(),
            prompt_draft: String::new(),
            selected_tool_index: None,
        }
    }

    pub fn ready(&mut self, provider: impl Into<String>) {
        self.provider = provider.into();
        self.status = AcpViewStatus::Ready;
    }

    pub fn set_session_id(&mut self, session_id: impl Into<String>) {
        self.session_id = Some(session_id.into());
    }

    pub fn submit_prompt(&mut self, line: &str) {
        self.transcript
            .push(TranscriptEntry::User(format!("You: {line}")));
        self.status = AcpViewStatus::Running;
        self.prompt_draft.clear();
    }

    pub fn append_agent_text(&mut self, text: &str) {
        if let Some(TranscriptEntry::AgentText(existing)) = self.transcript.last_mut() {
            existing.push_str(text);
        } else {
            self.transcript
                .push(TranscriptEntry::AgentText(text.to_owned()));
        }
        self.status = AcpViewStatus::Running;
    }

    pub fn append_terminal_raw(&mut self, text: &str) {
        self.transcript
            .push(TranscriptEntry::TerminalRaw(text.to_owned()));
        self.status = AcpViewStatus::Running;
    }

    pub fn apply_tool_update_v2(&mut self, update: &v2::ToolCallUpdate) {
        let id = update.tool_call_id.0.to_string();
        let title = match &update.title {
            MaybeUndefined::Value(title) => title.clone(),
            _ => "Tool activity".to_owned(),
        };
        let status = match &update.status {
            MaybeUndefined::Value(status) => tool_status_v2(status),
            _ => ToolStatus::InProgress,
        };
        let content = tool_content_v2(update);
        self.upsert_tool(id, title, status, content);
        self.status = AcpViewStatus::Running;
    }

    pub fn apply_tool_call_v1(&mut self, tool: &v1::ToolCall) {
        self.upsert_tool(
            tool.tool_call_id.0.to_string(),
            tool.title.clone(),
            ToolStatus::InProgress,
            None,
        );
        self.status = AcpViewStatus::Running;
    }

    pub fn apply_tool_update_v1(&mut self, update: &v1::ToolCallUpdate) {
        let id = update.tool_call_id.0.to_string();
        let title = update
            .fields
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Tool activity".to_owned());
        let status = update
            .fields
            .status
            .as_ref()
            .map(tool_status_v1)
            .unwrap_or(ToolStatus::InProgress);
        self.upsert_tool(id, title, status, None);
        self.status = AcpViewStatus::Running;
    }

    pub fn show_permission_v2(&mut self, request: &v2::RequestPermissionRequest) {
        self.show_permission(
            request.title.clone(),
            request.description.clone(),
            request.options.iter().map(|o| o.name.clone()).collect(),
        );
    }

    pub fn show_permission_v1(&mut self, request: &v1::RequestPermissionRequest) {
        let title = request
            .tool_call
            .fields
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Permission required".to_owned());
        self.show_permission(
            title,
            None,
            request.options.iter().map(|o| o.name.clone()).collect(),
        );
    }

    pub fn show_plain(&mut self, text: &str) {
        self.transcript
            .push(TranscriptEntry::Plain(text.to_owned()));
    }

    pub fn set_idle(&mut self) {
        self.status = AcpViewStatus::Ready;
    }

    pub fn set_waiting(&mut self) {
        self.status = AcpViewStatus::Waiting;
    }

    pub fn set_error(&mut self) {
        self.status = AcpViewStatus::Error;
    }

    pub fn toggle_selected_tool_expand(&mut self) {
        let Some(index) = self.selected_tool_index else {
            return;
        };
        if let Some(TranscriptEntry::Tool(tool)) = self.transcript.get_mut(index) {
            tool.expanded = !tool.expanded;
        }
    }

    pub fn select_next_tool(&mut self) {
        let indices = self.tool_entry_indices();
        if indices.is_empty() {
            return;
        }
        self.selected_tool_index = Some(match self.selected_tool_index {
            None => indices[0],
            Some(current) => {
                let pos = indices
                    .iter()
                    .position(|index| *index == current)
                    .unwrap_or(0);
                indices[(pos + 1) % indices.len()]
            }
        });
    }

    pub fn select_prev_tool(&mut self) {
        let indices = self.tool_entry_indices();
        if indices.is_empty() {
            return;
        }
        self.selected_tool_index = Some(match self.selected_tool_index {
            None => *indices.last().expect("non-empty tool indices"),
            Some(current) => {
                let pos = indices
                    .iter()
                    .position(|index| *index == current)
                    .unwrap_or(0);
                let prev = pos.checked_sub(1).unwrap_or(indices.len() - 1);
                indices[prev]
            }
        });
    }

    fn tool_entry_indices(&self) -> Vec<usize> {
        self.transcript
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if matches!(entry, TranscriptEntry::Tool(_)) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    fn show_permission(
        &mut self,
        title: String,
        description: Option<String>,
        options: Vec<String>,
    ) {
        self.transcript
            .push(TranscriptEntry::Permission(PermissionEntry {
                title,
                description,
                options,
            }));
        self.status = AcpViewStatus::Waiting;
    }

    fn upsert_tool(
        &mut self,
        id: String,
        title: String,
        status: ToolStatus,
        content: Option<String>,
    ) {
        if let Some(index) = self
            .transcript
            .iter()
            .position(|entry| matches!(entry, TranscriptEntry::Tool(tool) if tool.id == id))
        {
            if let TranscriptEntry::Tool(tool) = &mut self.transcript[index] {
                tool.title = title;
                tool.status = status;
                if content.is_some() {
                    tool.content = content;
                }
            }
            return;
        }
        self.transcript.push(TranscriptEntry::Tool(ToolEntry {
            id,
            title,
            status,
            content,
            expanded: false,
        }));
    }
}

pub(crate) fn should_expand_tool_on_enter(view: &AcpViewState) -> bool {
    view.selected_tool_index.is_some() && view.status != AcpViewStatus::Waiting
}

fn tool_status_v2(status: &v2::ToolCallStatus) -> ToolStatus {
    match status {
        v2::ToolCallStatus::Completed => ToolStatus::Completed,
        v2::ToolCallStatus::Failed => ToolStatus::Failed,
        _ => ToolStatus::InProgress,
    }
}

fn tool_status_v1(status: &v1::ToolCallStatus) -> ToolStatus {
    match status {
        v1::ToolCallStatus::Completed => ToolStatus::Completed,
        v1::ToolCallStatus::Failed => ToolStatus::Failed,
        _ => ToolStatus::InProgress,
    }
}

fn tool_content_v2(update: &v2::ToolCallUpdate) -> Option<String> {
    let MaybeUndefined::Value(content) = &update.content else {
        return None;
    };
    let mut parts = Vec::new();
    for item in content {
        match item {
            v2::ToolCallContent::Content(block) => {
                if let v2::ContentBlock::Text(text) = &block.content {
                    parts.push(text.text.clone());
                }
            }
            v2::ToolCallContent::Diff(diff) => {
                if let Some(patch) = &diff.patch {
                    parts.push(patch.text.clone());
                }
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn tool_glyph(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::InProgress => "●",
        ToolStatus::Completed => "✓",
        ToolStatus::Failed => "✗",
    }
}

fn status_label(status: AcpViewStatus) -> &'static str {
    match status {
        AcpViewStatus::Ready => "Ready",
        AcpViewStatus::Running => "Running",
        AcpViewStatus::Waiting => "Waiting",
        AcpViewStatus::Error => "Error",
    }
}

pub fn render_acp_view(frame: &mut Frame<'_>, state: &AcpViewState) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, chunks[0], state);
    render_transcript(frame, chunks[1], state);
    render_prompt(frame, chunks[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &AcpViewState) {
    let mut parts = vec![state.provider.clone()];
    if let Some(model) = &state.model {
        parts.push(model.clone());
    }
    parts.push(status_label(state.status).to_owned());
    if let Some(session_id) = &state.session_id {
        let short = if session_id.len() > 8 {
            session_id[session_id.len() - 8..].to_owned()
        } else {
            session_id.clone()
        };
        parts.push(short);
    }
    let header = parts.join(" · ");
    let widget = Paragraph::new(Line::from(Span::styled(
        header,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(widget, area);
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, state: &AcpViewState) {
    let lines = transcript_lines(state);
    let widget = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(Block::default().borders(Borders::TOP), area);
    frame.render_widget(widget, area);
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, state: &AcpViewState) {
    let prompt = if state.prompt_draft.is_empty() {
        "> ".to_owned()
    } else {
        format!("> {}", state.prompt_draft)
    };
    frame.render_widget(Paragraph::new(prompt), area);
}

fn transcript_lines(state: &AcpViewState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, entry) in state.transcript.iter().enumerate() {
        match entry {
            TranscriptEntry::User(text) => lines.push(Line::from(text.clone())),
            TranscriptEntry::AgentText(text) => lines.push(Line::from(text.clone())),
            TranscriptEntry::TerminalRaw(text) => lines.push(Line::from(text.clone())),
            TranscriptEntry::Plain(text) => lines.push(Line::from(text.clone())),
            TranscriptEntry::Tool(tool) => {
                let selected = state.selected_tool_index == Some(index);
                let marker = if selected { "›" } else { " " };
                let row = format!("{marker}{} {}", tool_glyph(tool.status), tool.title);
                let style = if selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(row, style)));
                if tool.expanded {
                    if let Some(content) = &tool.content {
                        for line in content.lines() {
                            lines.push(Line::from(format!("  {line}")));
                        }
                    }
                }
            }
            TranscriptEntry::Permission(permission) => {
                lines.push(Line::from(permission.title.clone()));
                if let Some(description) = &permission.description {
                    lines.push(Line::from(description.clone()));
                }
                let choices = permission
                    .options
                    .iter()
                    .map(|name| format!("[ {name} ]"))
                    .collect::<Vec<_>>()
                    .join("  ");
                lines.push(Line::from(choices));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v2;
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        let width = buffer.area.width;
        (0..width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| row_text(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_state(state: &AcpViewState) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render_acp_view(frame, state))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn ready_view_shows_header_status_and_sticky_prompt() {
        let mut state = AcpViewState::new("Cursor");
        state.ready("Cursor");
        let buffer = render_state(&state);
        let text = buffer_text(&buffer);
        assert!(text.contains("Cursor"));
        assert!(text.contains("Ready"));
        assert!(row_text(&buffer, 23).contains('>'));
        assert!(!text.contains("ACP ready"));
        assert!(!text.to_ascii_lowercase().contains("session/update"));
    }

    #[test]
    fn prompt_submit_echoes_you_and_sets_running() {
        let mut state = AcpViewState::new("Cursor");
        state.ready("Cursor");
        state.submit_prompt("fix race");
        let buffer = render_state(&state);
        let text = buffer_text(&buffer);
        assert!(text.contains("You: fix race"));
        assert!(text.contains("Running"));
    }

    #[test]
    fn tool_row_compact_glyphs_and_expand() {
        let mut state = AcpViewState::new("Cursor");
        state.ready("Cursor");
        state.apply_tool_update_v2(
            &v2::ToolCallUpdate::new("opaque-tool-id")
                .title("Compile project")
                .status(v2::ToolCallStatus::InProgress),
        );
        let in_progress = render_state(&state);
        let in_progress_text = buffer_text(&in_progress);
        assert!(in_progress_text.contains('●'));
        assert!(in_progress_text.contains("Compile project"));
        assert!(!in_progress_text.contains("opaque-tool-id"));

        state.apply_tool_update_v2(
            &v2::ToolCallUpdate::new("opaque-tool-id")
                .title("Compile project")
                .status(v2::ToolCallStatus::Completed)
                .content(vec![v2::ToolCallContent::Content(Box::new(
                    v2::Content::new(v2::ContentBlock::Text(v2::TextContent::new("build ok"))),
                ))]),
        );
        let completed = render_state(&state);
        assert!(buffer_text(&completed).contains('✓'));

        state.select_next_tool();
        state.toggle_selected_tool_expand();
        let expanded = render_state(&state);
        assert!(buffer_text(&expanded).contains("build ok"));

        state.toggle_selected_tool_expand();
        let collapsed = render_state(&state);
        assert!(!buffer_text(&collapsed).contains("build ok"));
    }

    #[test]
    fn enter_during_waiting_does_not_expand_tool() {
        let mut state = AcpViewState::new("Cursor");
        state.apply_tool_update_v2(
            &v2::ToolCallUpdate::new("tool-1")
                .title("Build")
                .status(v2::ToolCallStatus::InProgress),
        );
        state.select_next_tool();
        state.set_waiting();
        assert!(!should_expand_tool_on_enter(&state));
    }

    #[test]
    fn enter_expands_tool_when_running_with_selection() {
        let mut state = AcpViewState::new("Cursor");
        state.apply_tool_update_v2(
            &v2::ToolCallUpdate::new("tool-1")
                .title("Build")
                .status(v2::ToolCallStatus::InProgress),
        );
        state.select_next_tool();
        assert!(should_expand_tool_on_enter(&state));
    }

    #[test]
    fn permission_inline_choices_waiting() {
        let mut state = AcpViewState::new("Cursor");
        state.show_permission_v2(&v2::RequestPermissionRequest::new(
            "session",
            "Allow command: cargo test",
            vec![
                v2::PermissionOption::new(
                    "id-a",
                    "Allow once",
                    v2::PermissionOptionKind::AllowOnce,
                ),
                v2::PermissionOption::new("id-b", "Deny", v2::PermissionOptionKind::RejectOnce),
            ],
        ));
        let buffer = render_state(&state);
        let text = buffer_text(&buffer);
        assert!(text.contains("Allow command: cargo test"));
        assert!(text.contains("[ Allow once ]"));
        assert!(text.contains("[ Deny ]"));
        assert!(text.contains("Waiting"));
        assert!(!text.contains("id-a"));
        assert!(!text.contains("id-b"));
    }
}
