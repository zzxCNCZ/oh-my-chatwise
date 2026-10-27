# Project conventions

- Product name: `oh-my-chatwise` (lowercase, retain hyphens).
- All future UI work must follow [design/DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) and the user-supplied reference in `design/reference/`.
- Reuse `src/styles/tokens.css`, `src/styles/glass.css`, `AppIcon.vue` and `BrandLogo.vue`. Use the supplied brand slices; do not replace the logo with a generic star or redraw it.
- Preserve existing light/dark/system preferences. Forest Glass light is the default for new workspaces.
- Preserve current chat, model, attachment, tool, title, sync and configuration-transfer behavior while changing presentation. Do not add decorative controls that cannot perform their labeled action or invent account/subscription/provider states.
- Keep the application identifier, credential namespace and portable configuration format compatible when changing the display name.
- For UI changes, run `pnpm build` and `pnpm format:check`, and inspect the affected desktop/browser surfaces. Run relevant existing tests when behavior or defaults change. Keep test fixtures separate from user data.
