# Workspace-Window Relationship Tracking

## Current Implementation

The workspace-window relationship is currently tracked bidirectionally:

1. **Window → Workspace**: Each `ManagedWindow` has a `workspace: WorkspaceId` field
2. **Workspace → Windows**: Each `Workspace` has a `windows: Vec<WindowId>` field

## Maintaining Consistency

To keep these in sync, all workspace changes must go through:

1. **WorkspaceManager methods**:
   - `add_window_to_workspace()` - adds to workspace's window list
   - `remove_window_from_workspace()` - removes from workspace's window list
   - `move_window()` - handles both removal and addition atomically

2. **WindowRegistry/WindowManager methods**:
   - `set_workspace()` / `set_window_workspace()` - updates window's workspace field

## Best Practices

1. **Always use the manager methods** - Never directly modify the workspace or windows fields
2. **Atomic updates** - When moving windows, use `move_window()` to ensure both sides update
3. **Validation** - Check that window exists before updating relationships

## Future Improvements

A single source of truth approach could be implemented by:

1. **Option A**: Store relationship only in windows, derive workspace contents dynamically
   ```rust
   impl WorkspaceManager {
       pub fn windows_in_workspace(&self, id: WorkspaceId) -> Vec<WindowId> {
           self.window_registry.windows()
               .filter(|w| w.workspace == id)
               .map(|w| w.id)
               .collect()
       }
   }
   ```

2. **Option B**: Store relationship only in workspaces, lookup window's workspace dynamically
   ```rust
   impl WindowRegistry {
       pub fn window_workspace(&self, id: WindowId) -> Option<WorkspaceId> {
           self.workspace_manager.workspaces()
               .find(|ws| ws.windows.contains(&id))
               .map(|ws| ws.id)
       }
   }
   ```

The current bidirectional approach was chosen for performance (O(1) lookups in both directions) at the cost of needing careful synchronization.