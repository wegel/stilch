# Window Management Architecture

## Current State (Multiple Sources of Truth)

Windows are currently tracked in multiple places:

1. **WindowRegistry** (`HashMap<WindowId, ManagedWindow>`)
   - Source of truth for window metadata (workspace, layout state)
   - Provides WindowId -> ManagedWindow mapping

2. **Space<WindowElement>** (Smithay's spatial tracker)
   - Tracks window positions and stacking order
   - Stores full WindowElement copies
   - Used for rendering and input handling

3. **Workspace.windows** (`Vec<WindowId>`)
   - Maintains window order within workspace
   - Used for focus cycling

4. **LayoutTree** (Contains WindowIds in nodes)
   - Tracks tiling arrangement
   - Calculates window geometries

## Problems

- **Duplication**: WindowElements exist in both Space and Registry
- **Synchronization**: Changes must be propagated to multiple places
- **Inconsistency Risk**: Easy to update one but forget others
- **Memory Overhead**: Multiple copies of window data

## Proposed Solution

### Short Term (Pragmatic)
1. Add consistency checks in debug builds
2. Create a `WindowSyncGuard` that ensures updates happen atomically across all stores
3. Add invariant checks after each operation

### Long Term (Ideal)
1. Modify Space to store WindowIds instead of WindowElements
2. Make WindowRegistry the single source of truth
3. Have Space query Registry for window data when needed
4. Consolidate Workspace.windows into Registry (add ordering field)

## Invariants to Maintain

1. Every WindowElement in Space has a corresponding entry in Registry
2. Every WindowId in Workspace.windows exists in Registry
3. Window's workspace field matches the Workspace it's listed in
4. Window's geometry in Registry matches its position in Space
5. Only one window per workspace can be focused/fullscreen