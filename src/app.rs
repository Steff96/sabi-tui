//! Core application struct
//!
//! Contains the App struct that holds all application state.

use tui_textarea::TextArea;

use crate::config::Config;
use crate::message::Message;
use crate::state::{AppState, StateEvent, TransitionResult, transition};

/// Main application state container
pub struct App<'a> {
    /// Current application state
    pub state: AppState,

    /// Input textarea for user queries
    pub input_textarea: TextArea<'a>,

    /// Editable textarea for command review
    pub action_textarea: TextArea<'a>,

    /// Conversation history for AI context
    pub messages: Vec<Message>,

    /// Current command being executed
    pub current_command: Option<String>,

    /// Output from command execution
    pub execution_output: String,

    /// Error message if any
    pub error_message: Option<String>,

    /// Spinner frame for loading animation
    pub spinner_frame: usize,

    /// Flag to quit application
    pub should_quit: bool,

    /// Scroll offset for chat history
    pub scroll_offset: u16,

    /// Flag indicating dangerous command detected
    pub dangerous_command_detected: bool,

    /// Application configuration
    pub config: Config,
}

impl<'a> App<'a> {
    /// Create a new App instance with the given configuration
    pub fn new(config: Config) -> Self {
        let mut input_textarea = TextArea::default();
        input_textarea.set_placeholder_text("Type your query here...");
        
        let action_textarea = TextArea::default();

        Self {
            state: AppState::default(),
            input_textarea,
            action_textarea,
            messages: Vec::new(),
            current_command: None,
            execution_output: String::new(),
            error_message: None,
            spinner_frame: 0,
            should_quit: false,
            scroll_offset: 0,
            dangerous_command_detected: false,
            config,
        }
    }

    /// Get the current input text (trimmed)
    pub fn get_input_text(&self) -> String {
        self.input_textarea.lines().join("\n").trim().to_string()
    }

    /// Get the current action text (the command to execute)
    pub fn get_action_text(&self) -> String {
        self.action_textarea.lines().join("\n").trim().to_string()
    }

    /// Check if the input is empty (whitespace-only counts as empty)
    pub fn is_input_empty(&self) -> bool {
        self.get_input_text().is_empty()
    }

    /// Clear the input textarea
    pub fn clear_input(&mut self) {
        self.input_textarea = TextArea::default();
        self.input_textarea.set_placeholder_text("Type your query here...");
    }

    /// Clear the action textarea
    pub fn clear_action(&mut self) {
        self.action_textarea = TextArea::default();
        self.dangerous_command_detected = false;
    }

    /// Set the action textarea content (for command review)
    pub fn set_action_text(&mut self, text: &str) {
        self.action_textarea = TextArea::default();
        for line in text.lines() {
            self.action_textarea.insert_str(line);
            self.action_textarea.insert_newline();
        }
        // Remove the trailing newline if we added one
        if text.lines().count() > 0 {
            self.action_textarea.delete_char();
        }
    }

    /// Add a message to the conversation history
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Clear the error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Set an error message
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error_message = Some(error.into());
    }

    /// Attempt a state transition
    /// 
    /// Returns true if the transition was successful, false otherwise.
    pub fn transition(&mut self, event: StateEvent) -> bool {
        match transition(self.state, event) {
            TransitionResult::Success(new_state) => {
                self.state = new_state;
                true
            }
            TransitionResult::Ignored => false,
            TransitionResult::Error(msg) => {
                self.set_error(msg);
                false
            }
        }
    }

    /// Submit the current input
    /// 
    /// Returns true if the input was submitted (non-empty), false otherwise.
    pub fn submit_input(&mut self) -> bool {
        let is_empty = self.is_input_empty();
        
        if !is_empty {
            let input = self.get_input_text();
            self.add_message(Message::user(&input));
            self.clear_input();
        }
        
        self.transition(StateEvent::SubmitInput { is_empty })
    }

    /// Advance the spinner animation
    pub fn tick_spinner(&mut self) {
        const SPINNER_FRAMES: usize = 10;
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES;
    }

    /// Get the current spinner character
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Create a test App with default config
    fn test_app() -> App<'static> {
        App::new(Config::default())
    }

    // Strategy to generate whitespace-only strings
    fn whitespace_string() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof![Just(' '), Just('\t'), Just('\n'), Just('\r')], 0..20)
            .prop_map(|chars| chars.into_iter().collect())
    }

    // **Feature: agent-rs, Property 1: Empty Input Rejection**
    // *For any* input string composed entirely of whitespace characters, submitting it
    // SHALL NOT change the application state from Input, and the message history SHALL
    // remain unchanged.
    // **Validates: Requirements 1.3**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_empty_input_rejection(whitespace in whitespace_string()) {
            let mut app = test_app();
            
            // Ensure we start in Input state
            assert_eq!(app.state, AppState::Input);
            let initial_message_count = app.messages.len();
            
            // Set the input to whitespace-only content
            app.input_textarea = TextArea::default();
            for ch in whitespace.chars() {
                app.input_textarea.insert_char(ch);
            }
            
            // Attempt to submit
            let submitted = app.submit_input();
            
            // Property: submission should fail (return false)
            prop_assert!(!submitted, "Whitespace-only input should not be submitted");
            
            // Property: state should remain Input
            prop_assert_eq!(
                app.state, 
                AppState::Input,
                "State should remain Input after whitespace submission"
            );
            
            // Property: message history should be unchanged
            prop_assert_eq!(
                app.messages.len(),
                initial_message_count,
                "Message history should not change after whitespace submission"
            );
        }

        #[test]
        fn prop_empty_string_rejection(_dummy in 0..1) {
            let mut app = test_app();
            
            // Ensure we start in Input state with empty textarea
            assert_eq!(app.state, AppState::Input);
            let initial_message_count = app.messages.len();
            
            // Input is already empty by default
            assert!(app.is_input_empty());
            
            // Attempt to submit
            let submitted = app.submit_input();
            
            // Property: submission should fail
            prop_assert!(!submitted);
            
            // Property: state should remain Input
            prop_assert_eq!(app.state, AppState::Input);
            
            // Property: message history should be unchanged
            prop_assert_eq!(app.messages.len(), initial_message_count);
        }
    }

    #[test]
    fn test_is_input_empty_with_whitespace() {
        let mut app = test_app();
        
        // Empty by default
        assert!(app.is_input_empty());
        
        // Add spaces
        app.input_textarea.insert_str("   ");
        assert!(app.is_input_empty());
        
        // Add tabs
        app.clear_input();
        app.input_textarea.insert_str("\t\t");
        assert!(app.is_input_empty());
        
        // Add newlines
        app.clear_input();
        app.input_textarea.insert_str("\n\n");
        assert!(app.is_input_empty());
        
        // Add actual content
        app.clear_input();
        app.input_textarea.insert_str("hello");
        assert!(!app.is_input_empty());
    }

    // Strategy to generate non-empty, non-whitespace strings
    fn non_empty_string() -> impl Strategy<Value = String> {
        // Generate strings that have at least one non-whitespace character
        ("[a-zA-Z0-9][a-zA-Z0-9 ]{0,50}", 1..52)
            .prop_map(|(s, _)| s)
    }

    // **Feature: agent-rs, Property 2: Valid Input State Transition**
    // *For any* non-empty, non-whitespace input string, submitting it in Input state
    // SHALL transition the application to Thinking state and add the input to message history.
    // **Validates: Requirements 1.2**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_valid_input_state_transition(input in non_empty_string()) {
            let mut app = test_app();
            
            // Ensure we start in Input state
            assert_eq!(app.state, AppState::Input);
            let initial_message_count = app.messages.len();
            
            // Set the input to non-empty content
            app.input_textarea = TextArea::default();
            app.input_textarea.insert_str(&input);
            
            // Verify input is not empty
            prop_assert!(!app.is_input_empty(), "Input should not be empty: '{}'", input);
            
            // Attempt to submit
            let submitted = app.submit_input();
            
            // Property: submission should succeed (return true)
            prop_assert!(submitted, "Non-empty input should be submitted");
            
            // Property: state should transition to Thinking
            prop_assert_eq!(
                app.state, 
                AppState::Thinking,
                "State should transition to Thinking after valid submission"
            );
            
            // Property: message history should have one more message
            prop_assert_eq!(
                app.messages.len(),
                initial_message_count + 1,
                "Message history should grow by 1 after valid submission"
            );
            
            // Property: the new message should be a User message with the input content
            let last_message = app.messages.last().unwrap();
            prop_assert_eq!(last_message.role.clone(), crate::message::MessageRole::User);
            prop_assert_eq!(last_message.content.trim(), input.trim());
        }

        #[test]
        fn prop_input_cleared_after_submission(input in non_empty_string()) {
            let mut app = test_app();
            
            // Set the input
            app.input_textarea = TextArea::default();
            app.input_textarea.insert_str(&input);
            
            // Submit
            app.submit_input();
            
            // Property: input should be cleared after submission
            prop_assert!(
                app.is_input_empty(),
                "Input should be cleared after submission"
            );
        }
    }

    #[test]
    fn test_valid_input_submission() {
        let mut app = test_app();
        
        // Set valid input
        app.input_textarea.insert_str("list files");
        
        // Submit
        let submitted = app.submit_input();
        
        assert!(submitted);
        assert_eq!(app.state, AppState::Thinking);
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].content, "list files");
    }

    // Strategy to generate arbitrary error messages
    fn arb_error_message() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,100}".prop_map(|s| s)
    }

    // **Feature: agent-rs, Property 5: API Error Recovery**
    // *For any* API error during Thinking state, the application SHALL transition
    // back to Input state and set an error message.
    // **Validates: Requirements 2.4**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_api_error_recovery_from_thinking(error_msg in arb_error_message()) {
            let mut app = test_app();
            
            // First, get to Thinking state by submitting valid input
            app.input_textarea.insert_str("test query");
            app.submit_input();
            
            // Verify we're in Thinking state
            prop_assert_eq!(app.state, AppState::Thinking);
            
            // Simulate API error by setting error and transitioning
            app.set_error(&error_msg);
            let transitioned = app.transition(StateEvent::ApiError);
            
            // Property: transition should succeed
            prop_assert!(transitioned, "API error transition should succeed from Thinking state");
            
            // Property: state should be Input after API error
            prop_assert_eq!(
                app.state,
                AppState::Input,
                "State should transition to Input after API error"
            );
            
            // Property: error message should be set
            prop_assert!(
                app.error_message.is_some(),
                "Error message should be set after API error"
            );
            
            // Property: error message should match what we set
            prop_assert_eq!(
                app.error_message.as_ref().unwrap(),
                &error_msg,
                "Error message should match the set error"
            );
        }

        #[test]
        fn prop_api_error_recovery_from_finalizing(error_msg in arb_error_message()) {
            let mut app = test_app();
            
            // Manually set state to Finalizing (simulating post-command execution)
            app.state = AppState::Finalizing;
            
            // Simulate API error
            app.set_error(&error_msg);
            let transitioned = app.transition(StateEvent::ApiError);
            
            // Property: transition should succeed
            prop_assert!(transitioned, "API error transition should succeed from Finalizing state");
            
            // Property: state should be Input after API error
            prop_assert_eq!(
                app.state,
                AppState::Input,
                "State should transition to Input after API error in Finalizing"
            );
            
            // Property: error message should be set
            prop_assert!(
                app.error_message.is_some(),
                "Error message should be set after API error"
            );
        }

        #[test]
        fn prop_api_error_preserves_message_history(
            input in non_empty_string(),
            error_msg in arb_error_message()
        ) {
            let mut app = test_app();
            
            // Submit input to get to Thinking state
            app.input_textarea.insert_str(&input);
            app.submit_input();
            
            // Record message count after submission
            let message_count = app.messages.len();
            
            // Simulate API error
            app.set_error(&error_msg);
            app.transition(StateEvent::ApiError);
            
            // Property: message history should be preserved (not cleared)
            prop_assert_eq!(
                app.messages.len(),
                message_count,
                "Message history should be preserved after API error"
            );
        }
    }

    #[test]
    fn test_api_error_recovery() {
        let mut app = test_app();
        
        // Get to Thinking state
        app.input_textarea.insert_str("test");
        app.submit_input();
        assert_eq!(app.state, AppState::Thinking);
        
        // Simulate API error
        app.set_error("Network error");
        app.transition(StateEvent::ApiError);
        
        // Should be back in Input state with error
        assert_eq!(app.state, AppState::Input);
        assert!(app.error_message.is_some());
        assert_eq!(app.error_message.unwrap(), "Network error");
    }
}
