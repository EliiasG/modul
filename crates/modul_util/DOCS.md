# modul_util

Small utilities and convenience plugins.

## ExitPlugin

Automatically exits the application when the main window receives a close request. Add to your app during setup.

## Utilities

### binsearch

```rust
binsearch(|x| condition(x), start..end)
```

Binary search to find valid parameter values within a range.

## Re-exports

- `HashMap` - from hashbrown
- `HashSet` - from hashbrown

Using hashbrown provides consistent hash behavior across the engine.
