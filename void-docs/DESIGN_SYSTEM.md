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
- Welcome Screen Logo:
    - Box: Square container with 3px border
    - SVG Scale: rems(3.5)
    - Headline: "/void" (XLarge, ExtraBold)
- Tagline: "THINK. BUILD. SHIP." (Uppercase, ExtraBold, Small, Amber)

## 3. Welcome Screen Component
- Container:
    - Max Width: rems_from_px(512.)
    - Frame: 3px solid border
    - Shadow: 20px solid black offset
- Navigation Grid:
    - Layout: 2-column grid
    - Item Height: h_20()
    - Border: 3px outer, 1px gap separators
    - Hover State: Background Amber, Text Black
- Typography: All labels Uppercase, ExtraBold

## 4. AI Setup & Onboarding
- Card Structure:
    - Frame: 1px rounded-md border
    - Featured Card Tint: rgba(0xffb0000d) (Amber at 5% opacity)
- Call to Action:
    - Shape: Rounded-full (pill)
    - Style: Subtle
    - Highlight: Color::Accent (Amber) for recommended providers

## 5. Icon Standards (SVG)
All agent icons use the `[/]` brand mark with the following font properties:
- Font: ui-monospace, FontWeight 900
- void_agent.svg (Logo): font-size="80", letter-spacing="-5"
- void_agent_toggle.svg (Status Bar): font-size="62", letter-spacing="-6"
- void_mark.svg (Sidebar): Standard bold text mark

## Technical Implementation Notes (GPUI/UI)

- **Typography**: Use `Label` instead of `Headline` for brand text if you need to set `.font_weight()`. `Headline` is a specialized component that does not expose weight methods.
- **Shadows**: Correct `BoxShadow` syntax requires a raw `Vec` (no `Option`), and the `offset` must use the `point(px(x), px(y))` helper function.
- **Brutalist Buttons**: The high-level `Button` component enforces rounded corners. For sharp industrial corners, use `ButtonLike` with a `div()` child.
- **Color Traits**: When using `.border_color()` or `.bg()`, use `cx.theme().colors()` or `gpui::rgba()` directly. The `ui::Color` enum often lacks the necessary `Into<Hsla>` traits for these methods.

## 6. Layout Principles
- No Tables: All documentation and simple UI lists must use bullet points
- Case: Use SNAKE_CASE or UPPERCASE for technical labels to mimic config files
- Geometry: Favor sharp rectangles over rounded corners (except for pill buttons)
