# /void Design System — Brutalist UI Guide

This document defines the core visual tokens and component styles for the /void editor rebrand.

## 1. Core Visual Tokens
- Accent Color: Void Amber (#ffb000)
- Background: High-contrast Dark (#0a0a0a)
- Borders: Industrial Thick (3px for primary, 1px for secondary)
- Border Color: gpui::Color::Default (White in dark mode)
- Shadows: Solid Industrial Offset (20px black, 0 blur)

## 2. Brand Mark & Logo
- Type: Text-based "Badge" mark
- Welcome Screen Header:
    - Logo: Geometric `[/]void` text-based mark
    - Headline: "void" (ExtraBold, XLarge)
    - Tagline: "THINK. BUILD. SHIP." (90° Rotated, Amber, ExtraBold, Small)

- Layout: 3-column split header (Logo - Spacer - Tagline)
- Container:
    - Width: rems(36.)
    - Border: 1px solid
    - Shadow: 20px solid black offset
- Navigation Grid:
    - Layout: 2x2 grid for main actions
    - Item Style: Stacked (Title, Description, Boxed Keybinding)
    - Border: 1px internal separators
- Agent Card:
    - Style: Industrial card with 1px border
    - Primary CTA: Amber shadowed button (gpui::black text)
- Footer: 3-column grid for secondary actions (CLONE_REPO, etc.)

## 4. AI Setup & Onboarding
- Card Structure:
    - AI Setup: Large padding (`p_4`), wider gap (`gap_2`), and 1px rounded-md border.
    - Agent Setup: Medium padding (`p_3`).
    - Featured Card Tint: rgba(0xffb0000d) (Amber at 5% opacity).
    - Interaction: Border color shifts to `colors.border` on hover.
- Call to Action:
    - Shape: Rounded-full (pill)
    - Style: Subtle
    - Highlight: Color::Accent (Amber) for recommended providers

## 5. Icon Standards (SVG)
All agent icons use the `[/]` brand mark. To ensure consistency and specific "thickness", these are implemented as manual geometric paths.

- **Design Specification (100x100 viewBox)**:
    - **Brackets**: Sharp rectangular paths. M18 20 H33 V30 H28 V70 H33 V80 H18 Z.
    - **Slash**: Rounded line (`stroke-linecap="round"`) with `stroke-width="12"`.
    - **Proportions**: Slash is shorter than brackets (y=30 to 70 vs y=20 to 80).
    - **Spacing**: Close but distinct (~3px gap).
- **Variants**:
    - `void_agent.svg`: Primary logo.
    - `void_agent_toggle.svg`: Status bar variant.
    - `void_agent_two.svg`: Offset main mark with a bold "2" at bottom right.
    - `void_mark.svg`: 16x16 scaled version (use stroke-width="2" for slash).

## Technical Implementation Notes (GPUI/UI)

- **Typography**: Use `Label` instead of `Headline` for brand text if you need to set `.font_weight()`. `Headline` is a specialized component that does not expose weight methods.
- **Shadows**: Correct `BoxShadow` syntax requires a raw `Vec` (no `Option`), and the `offset` must use the `point(px(x), px(y))` helper function.
- **Brutalist Buttons**: The high-level `Button` component enforces rounded corners. For sharp industrial corners, use `ButtonLike` with a `div()` child.
- **Color Traits**: When using `.border_color()` or `.bg()`, use `cx.theme().colors()` or `gpui::rgba()` directly. The `ui::Color` enum often lacks the necessary `Into<Hsla>` traits for these methods.

## 7. Technical Best Practices & Pitfalls

To ensure the /void codebase remains stable and compilable, follow these technical guidelines when working with GPUI.

### Avoid "API Hallucinations"
GPUI's styling DSL is primarily generated via macros. **Do not assume an API exists just because it exists in CSS or other frameworks.**

- **Non-existent Methods**:
    - `.w_fit()`: Use `.w_auto()` or leave width unset (default is fit-to-content).
    - `gpui::Degrees(n)`: This type does not exist. Use `gpui::Radians(f32)` for all rotations.
- **Verification Strategy**:
    - To check if a styling method (like `.p_4()` or `.w_full()`) exists, check `crates/gpui_macros/src/styles.rs` in the `box_prefixes` and `box_style_suffixes` functions.
    - Use `rg -n "fn <method_name>"` in `crates/gpui/src/` to find actual trait implementations.

### Working with `AnyElement`
When using conditional rendering (`when`, `when_some`), be careful with variable types.
- **Shadowing Danger**: Avoid naming a local `AnyElement` the same as a static or data-bearing struct (e.g., naming a rendered section `second_section` if `second_section` is also your data source). This leads to "field doesn't exist on AnyElement" errors.
- **Conversion**: Use `.into_any_element()` only when necessary (e.g., for storing different element types in an `Option` or `Vec`).

### Animation & Easing
- **Blinking Effects**: Use `Animation::new(Duration)` with `pulsating_between(min, max)` easing for brand-consistent breathing animations.
- **Visibility**: Use `.opacity(delta)` within the animator closure rather than trying to modify colors directly for better performance and stability.

### Geometry
- **Radians**: All rotation transforms require `gpui::Radians`.
    - 90 degrees = `gpui::Radians(std::f32::consts::PI / 2.0)`
    - 180 degrees = `gpui::Radians(std::f32::consts::PI)`
