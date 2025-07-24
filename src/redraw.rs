use std::time::Instant;
use smithay::output::Output;
use std::collections::HashMap;

/// State tracking redraw needs for an output
#[derive(Debug, Clone)]
pub struct OutputRedrawState {
    /// Whether this output needs to be redrawn
    pub needs_redraw: bool,
    /// Last time this output was rendered
    pub last_render_time: Option<Instant>,
}

impl Default for OutputRedrawState {
    fn default() -> Self {
        Self {
            needs_redraw: false,
            last_render_time: None,
        }
    }
}

/// Manages redraw state for all outputs
#[derive(Debug, Default)]
pub struct RedrawScheduler {
    /// Per-output redraw state
    output_state: HashMap<Output, OutputRedrawState>,
}

impl RedrawScheduler {
    /// Create a new redraw scheduler
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an output to track
    pub fn add_output(&mut self, output: Output) {
        self.output_state.entry(output).or_default();
    }
    
    /// Remove an output from tracking
    pub fn remove_output(&mut self, output: &Output) {
        self.output_state.remove(output);
    }
    
    /// Queue a redraw for an output
    pub fn queue_redraw(&mut self, output: &Output) {
        if let Some(state) = self.output_state.get_mut(output) {
            state.needs_redraw = true;
        }
    }
    
    /// Queue redraws for all outputs
    pub fn queue_redraw_all(&mut self) {
        for state in self.output_state.values_mut() {
            state.needs_redraw = true;
        }
    }
    
    /// Check if an output needs redraw
    pub fn needs_redraw(&self, output: &Output) -> bool {
        self.output_state
            .get(output)
            .map(|s| s.needs_redraw)
            .unwrap_or(false)
    }
    
    /// Clear redraw flag for an output (called after rendering)
    pub fn clear_redraw(&mut self, output: &Output) {
        if let Some(state) = self.output_state.get_mut(output) {
            state.needs_redraw = false;
            state.last_render_time = Some(Instant::now());
        }
    }
    
    /// Get outputs that need redraw
    pub fn outputs_needing_redraw(&self) -> Vec<Output> {
        self.output_state
            .iter()
            .filter_map(|(output, state)| {
                if state.needs_redraw {
                    Some(output.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}