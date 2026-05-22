# Agent Project Instructions: Vapor (Godot 4.x)

This file provides context and best practices for Gemini CLI agents working on the "Vapor" Godot project.

## Project Context
- **Engine:** Godot 4.6 (Forward Plus renderer).
- **Language:** GDScript 2.0.

## Coding Standards & Best Practices

### 1. Style & Formatting
- **Indentation:** Always use **Tabs** (standard Godot convention).
- **Naming:** 
  - **Files:** `snake_case.gd`, `snake_case.tscn`.
  - **Functions/Variables:**  `snake_case` .
  - **Signals:** `snake_case` .
  - **Constants:** `SCREAMING_SNAKE_CASE`.
  - **Classes:** `PascalCase`.
- **Typing:** Use static typing where possible for performance and clarity.

### 2. Architecture & Communication
- **Decoupling:** Prefer signals over direct node references (`get_parent().get_node(...)`) to keep systems modular.
- **Resources:** Use `.tres` files for data-driven design.

## Workflow Instructions for Agents

### 1. Investigative Steps
- When receiving a prompt from the user relating to a new feature, start by putting a "project manager hat". Before working on anything, the first step is to always make sure we have a ticket, an implementation plan, and a task list.
- The ticket will be appended to the end of tickets.md. A template for tickets should be at the top of the file; ask the user if any information needed is misssing to create a ticket.
- The implementation plan should be a new dedicated file in the same directory as the tickets.md. A template file should already exist in there to inform you on how to build it
- A task list is created. This task list is used to keep the Ai-agent on target. As the coding agent addresses this in the future (when it's time to implement), they must update the task as they go, so that if they get interrupted, a new agent knows what was already done.



### 2. Implementation Rules
- Consider the overall architecture first, given the ticket, the implementation plan, and the task list.
- We do TDD. If there is a GUT addon, use it to create failing tests
- Loop the implementation until the tests pass
- Review everything to make sure the code is clean, readable, and there are no vulnerabilities for future bugs.
- Ensure the feature is workable with the larger project context

### 3. Verification
- Use `run_command` to run headless tests if the project has them (check `addons/` for testing frameworks).
- Make sure your findings are documented in a dedicated "walkthrough" document, taking the walkthrough template file as a basis.
- Announce that the implementation is complete.

## Feature Development Workflow

### 1. Task Tracking (`tasks.md`)
- **Maintain a `tasks.md` File:** For large projects or features, ensure a `tasks.md` exists in the `docs/` directory.
- **Purpose:** Use it to outline the active "to-do" sequence. This helps the AI stay on target and provides a clear roadmap.
- **Session Reuse:** On subsequent new sessions, this file should be updated, cleared, or repurposed to reflect the current session's objectives.

### 2. Implementation Planning (`implementation_plan.md`)
- **Mandatory for Complex Features:** Create or repurpose an `implementation_plan.md` before starting significant changes.
- **Required Sections:**
  - **Title:** High-level summary of the feature request.
  - **User Story:** "As a [user], I want to [action] so that [benefit]."
  - **Context:** Description of the current state and relevant existing code.
  - **Description:** Clear summary of what needs to change.
  - **Requirements:** Prerequisites or dependencies that must be met before starting.
  - **Proposed Changes:** A step-by-step list of all modifications. Use the format:
	- `[modify] filename.ext`
	- `[create] filename.ext`
	- `[destroy] filename.ext`
  - **Verification Plan:** Define acceptance criteria using **Gherkin** language (Given/When/Then) to outline manual tests for verification.
- **Mandatory Confirmation:** After creating the `tasks.md` and `implementation_plan.md`, **you must wait** for the user to explicitly confirm the plan or provide feedback/improvements. Do not begin the implementation (modifying or creating files) until the plan has been approved.

### 3. Feature Walkthrough (`walkthrough.md`)
- **Documentation on Completion:** Once a feature is developed and verified, create a `walkthrough.md` for the user.
- **Required Sections:**
  - **Summary:** Overview of what was completed.
  - **Verified Requirements:** Confirmation that the pre-implementation requirements were met.
  - **Successful Changes:** List of all major changes made.
  - **Out of Scope / Unaddressed:** Document any items not in the original scope or that need future attention.
  - **Verification Results:** Clear evidence or logs showing that the manual tests (from the implementation plan) passed.

## CLI Interaction & Session Management

### 1. Handling Vague Prompts
- **Identify & Call Out:** If a user prompt is vague, ambiguous, or lacks sufficient detail to act safely (e.g., "fix the bug" without specifying which bug), **do not guess**.
- **Ask for Clarification:** Politely call out the vagueness and ask the user to be more specific. 
- **Prompt Best Practices:** Remind the user of prompt engineering best practices:
  - Define the **Goal** clearly.
  - Provide **Context** (file paths, specific functions, or error logs).
  - Specify **Constraints** or desired outcomes.
  - *Example Response:* "This request is a bit vague. To help you better, could you specify which script you are referring to and what the expected behavior should be? A good prompt includes the 'What', 'Where', and 'Why'."

### 2. Context Window Management
- **Use `/clear` for New Contexts:** When a user pivots to a completely unrelated feature or project (e.g., switching from 'Player Movement' to 'UI Sound Settings' with no logical connection), suggest using the `/clear` command. Explain that this prevents 'context pollution' and ensures the AI doesn't get confused by previous, irrelevant code snippets.
- **Suggest `/compact` for Long Sessions:** In extended sessions working on the same feature, context can become bloated.
  - **When to suggest:** After approximately **15-20 turns** or if the session history is becoming very long (approaching high token counts).
  - **Action:** Instead of answering the next request immediately, suggest: "We've been working on this for a while. To keep our context window efficient and avoid potential confusion, would you like to use `/compact` (or `/summarize`) before we continue?"

### 3. Model Selection Advice
- **Analyze Request Complexity:** For every prompt, evaluate if the current model is the most efficient and capable choice for the task.
- **Suggest Alternatives:** If there is a clear mismatch, suggest the user switch models before you provide a full answer.
  - **Simple/Quick Tasks:** If the user is on a "Heavy" model but asking simple questions (e.g., "What does this function do?"), suggest switching to a **Fast** model to save time and tokens.
  - **Complex/Architectural Tasks:** If the user is on a "Fast" model but requesting a complex feature or major refactor, suggest switching to a **Heavy** (more capable) model to ensure higher quality and better reasoning.
  - *Example Response:* "This task involves complex architectural changes across multiple files. To ensure the best results and avoid logic errors, I recommend switching to a more capable 'Heavy' model before we proceed."

### 4. Scope & Goal Analysis
- **Single Objective Principle:** Features or projects should be constrained to a single, clear objective for the best quality results.
- **Analyze for Multi-Goal Prompts:** If a prompt is too broad, obtuse, or attempts to tackle multiple complex systems at once:
  - **Do not proceed immediately.**
  - **Advise the User:** Inform them that the prompt's scope is too large or multi-faceted to be handled at maximum quality.
  - **Suggest a Narrower Scope:** Propose a specific, single clear goal to start with (e.g., "Instead of refactoring the entire UI and adding a new inventory system, should we start by just implementing the base UI menu class?").
  - **Wait for Confirmation:** Once the user confirms the reduced scope, proceed with the standard workflow (creating `tasks.md`, `implementation_plan.md`, etc.).

## AI Assistant Integration
- This project includes `addons/ai_assistant_hub`, which connects to a local LLM (Ollama by default). 
- When working on tools, consider how they might integrate with this hub.

---
*Created on 2026-03-28. Update this file as project conventions evolve.*
