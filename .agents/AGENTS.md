# Project Rules

- For Godot projects, `.tscn` files with UI created dynamically as scenes should be preferred over injecting UI elements through code.
- **GDScript type annotations**: Always use explicit type annotations (e.g. `var x: float = ...`) when declaring variables with ternary expressions (`a if cond else b`). GDScript cannot infer the type from a ternary when the two branches differ in how their types are resolved (e.g. a property access vs. a literal), which causes a "Cannot infer the type of variable" compile error. Never use `:=` with ternary expressions.
