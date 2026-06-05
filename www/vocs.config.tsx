import React from 'react'
import { defineConfig } from 'vocs'

const headerCss = `
  /* Vocs's default top nav is [Search] [Logo] [empty spacer] [Nav items]
     which puts the search on the far left and the nav on the far right,
     leaving a large visual gap in the middle. We want a more conventional
     docs layout: [Logo + Nav] on the left, [Search] on the right next to
     the theme toggle. */
  .vocs_DesktopTopNav {
    gap: 12px;
  }
  /* Drop the empty spacer section that sits between the logo and the nav. */
  .vocs_DesktopTopNav > .vocs_DesktopTopNav_section:empty {
    display: none;
  }
  /* Reorder: send the search bar to the end so space-between puts the nav
     items on the left and the search on the right. */
  .vocs_DesktopTopNav > .vocs_DesktopSearch_search {
    order: 99;
    width: auto;
    min-width: 220px;
    max-width: 280px;
  }

  /* === Landing page (monochrome — gray accents instead of blue) === */
  .vocs_Content .landing-root {
    max-width: 1100px;
    margin: 0 auto;
    padding: 24px 0 96px;
  }
  .vocs_Content .landing-eyebrow {
    text-align: center;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--vocs-color_text2);
    margin-bottom: 16px;
  }
  .vocs_Content .landing-headline {
    text-align: center;
    font-size: clamp(40px, 6vw, 64px);
    font-weight: 600;
    line-height: 1.05;
    letter-spacing: -0.02em;
    margin: 0 0 20px;
  }
  .vocs_Content .landing-headline em {
    font-style: normal;
    color: var(--vocs-color_text2);
  }
  .vocs_Content .landing-tagline {
    text-align: center;
    font-size: clamp(15px, 1.4vw, 18px);
    line-height: 1.6;
    color: var(--vocs-color_text2);
    max-width: 640px;
    margin: 0 auto 28px;
  }
  .vocs_Content .landing-ctas {
    display: flex;
    gap: 12px;
    justify-content: center;
    flex-wrap: wrap;
    margin-bottom: 56px;
  }
  .vocs_Content .landing-cta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 500;
    text-decoration: none;
    border: 1px solid var(--vocs-color_border);
    color: var(--vocs-color_text);
    background: var(--vocs-color_background);
    transition: border-color 0.15s, background 0.15s, color 0.15s;
  }
  .vocs_Content .landing-cta:hover {
    border-color: var(--vocs-color_border2);
    color: var(--vocs-color_text);
  }
  .vocs_Content .landing-cta-primary {
    background: var(--vocs-color_text);
    border-color: var(--vocs-color_text);
    color: var(--vocs-color_background);
  }
  .vocs_Content .landing-cta-primary:hover {
    background: var(--vocs-color_text2);
    border-color: var(--vocs-color_text2);
    color: var(--vocs-color_background);
  }

  .vocs_Content .landing-section {
    margin: 72px 0 0;
  }
  .vocs_Content .landing-section-title {
    text-align: center;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--vocs-color_text3);
    margin-bottom: 28px;
  }

  .vocs_Content .landing-features {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
  }
  @media (max-width: 720px) {
    .vocs_Content .landing-features { grid-template-columns: 1fr; }
  }
  .vocs_Content .landing-feature {
    border: 1px solid var(--vocs-color_border);
    border-radius: 12px;
    padding: 24px;
    background: var(--vocs-color_background);
    text-align: left;
  }
  .vocs_Content .landing-feature-value {
    font-size: 32px;
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    color: var(--vocs-color_text);
  }
  .vocs_Content .landing-feature-label {
    margin-top: 6px;
    font-size: 14px;
    color: var(--vocs-color_text2);
    line-height: 1.4;
  }

  .vocs_Content .landing-flow {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0;
    border: 1px solid var(--vocs-color_border);
    border-radius: 12px;
    overflow: hidden;
  }
  @media (max-width: 720px) {
    .vocs_Content .landing-flow { grid-template-columns: 1fr; }
  }
  .vocs_Content .landing-flow-step {
    padding: 28px 24px;
    text-align: left;
    border-right: 1px solid var(--vocs-color_border);
  }
  .vocs_Content .landing-flow-step:last-child { border-right: none; }
  @media (max-width: 720px) {
    .vocs_Content .landing-flow-step { border-right: none; border-bottom: 1px solid var(--vocs-color_border); }
    .vocs_Content .landing-flow-step:last-child { border-bottom: none; }
  }
  .vocs_Content .landing-flow-num {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.1em;
    color: var(--vocs-color_text2);
    margin-bottom: 8px;
  }
  .vocs_Content .landing-flow-title {
    font-size: 18px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .vocs_Content .landing-flow-desc {
    font-size: 14px;
    color: var(--vocs-color_text2);
    line-height: 1.5;
  }

  .vocs_Content .landing-langs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: center;
  }
  .vocs_Content .landing-lang {
    font-size: 12px;
    font-weight: 500;
    padding: 5px 10px;
    border: 1px solid var(--vocs-color_border);
    border-radius: 999px;
    color: var(--vocs-color_text2);
    background: var(--vocs-color_background);
  }

  .vocs_Content .landing-foot {
    margin-top: 80px;
    text-align: center;
    font-size: 12px;
    color: var(--vocs-color_text3);
    letter-spacing: 0.05em;
  }
  .vocs_Content .landing-foot a { color: var(--vocs-color_text2); }
  .vocs_Content .landing-foot a:hover { color: var(--vocs-color_text); }
`

export default await defineConfig({
  title: 'Lexa',
  description: 'Fast local code intelligence for humans and AI agents.',
  rootDir: 'src',
  ogImageUrl: null,
  // Vocs 1.4.1's `typeof config.head === 'object'` check misidentifies a
  // plain ReactElement (which is also an object) and then returns undefined
  // from the path-keyed branch. Wrapping in a function sidesteps the check.
  head: () => <style>{headerCss}</style>,
  vite: {
    // fsevents is a macOS-only native module pulled in transitively by vocs.
    // Vite tries to bundle the .node binary as JS, which fails. Exclude it.
    optimizeDeps: {
      exclude: ['fsevents'],
    },
    ssr: {
      external: ['fsevents'],
    },
    build: {
      rollupOptions: {
        external: ['fsevents'],
      },
    },
  },
  topNav: [
    { text: 'Home', link: '/' },
    { text: 'Docs', link: '/docs/quick-start' },
    { text: 'Changelog', link: '/docs/changelog' },
    { text: 'GitHub', link: 'https://github.com/anvia-hq/lexa' },
  ],
  sidebar: [
    {
      text: 'Introduction',
      items: [
        { text: 'Welcome', link: '/' },
        { text: 'Why Lexa', link: '/docs/why-lexa' },
      ],
    },
    {
      text: 'Getting Started',
      items: [
        { text: 'Quick Start', link: '/docs/quick-start' },
        { text: 'Install', link: '/docs/install' },
        { text: 'Project Graph', link: '/docs/project-graph' },
      ],
    },
    {
      text: 'CLI',
      items: [
        { text: 'Overview', link: '/docs/cli' },
        { text: 'Indexing', link: '/docs/cli/indexing' },
        { text: 'Discovery', link: '/docs/cli/discovery' },
        { text: 'Search', link: '/docs/cli/search' },
        { text: 'Code Structure', link: '/docs/cli/code-structure' },
        { text: 'Read & Edit', link: '/docs/cli/read-edit' },
        { text: 'Audit', link: '/docs/cli/audit' },
        { text: 'Pipeline', link: '/docs/cli/pipeline' },
        { text: 'Maintenance', link: '/docs/cli/maintenance' },
      ],
    },
    {
      text: 'MCP',
      items: [
        { text: 'Setup', link: '/docs/mcp' },
        { text: 'Tools Reference', link: '/docs/mcp/tools' },
        { text: 'Output Formats', link: '/docs/mcp/output' },
      ],
    },
    {
      text: 'Reference',
      items: [
        { text: 'Global Flags', link: '/docs/global-flags' },
        { text: 'Configuration & Env Vars', link: '/docs/configuration' },
        { text: 'Language Support', link: '/docs/language-support' },
        { text: 'Safe-Edit Semantics', link: '/docs/safe-edit' },
      ],
    },
    {
      text: 'Project',
      items: [
        { text: 'Development', link: '/docs/development' },
        { text: 'Binary Releases', link: '/docs/binary-releases' },
        { text: 'Benchmark', link: '/docs/benchmark' },
        { text: 'Changelog', link: '/docs/changelog' },
      ],
    },
  ],
})
