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
  @media (max-width: 720px) {
    html,
    body {
      overflow-x: hidden;
    }
    .vocs_DocsLayout_gutterTop,
    .vocs_MobileTopNav {
      max-width: 100vw;
      overflow: hidden;
    }
  }

  /* === Landing page === */
  .vocs_Content,
  .vocs_Content > *,
  .vocs_DocsLayout_content,
  .vocs_DocsLayout {
    background: transparent !important;
  }
  table,
  .vocs_Content table,
  .vocs table,
  .vocs_Table {
    width: 100% !important;
    display: table !important;
  }
  :root {
    --lexa-primary: #e5ff1f !important;
    --vocs-color_textAccent: #e5ff1f !important;
    --vocs-color_textAccentHover: #d4f00e !important;
    --vocs-color_codeInlineText: #e5ff1f !important;
    --vocs-color_codeInlineBackground: color-mix(in srgb, #e5ff1f 8%, transparent) !important;
    --vocs-color_codeInlineBorder: color-mix(in srgb, #e5ff1f 15%, transparent) !important;
  }
  :root.dark {
    --vocs-color_textAccent: #e5ff1f !important;
    --vocs-color_textAccentHover: #d4f00e !important;
    --vocs-color_codeInlineText: #e5ff1f !important;
    --vocs-color_codeInlineBackground: color-mix(in srgb, #e5ff1f 8%, transparent) !important;
    --vocs-color_codeInlineBorder: color-mix(in srgb, #e5ff1f 15%, transparent) !important;
  }
  .vocs_Sidebar_item[data-active=true],
  .vocs_Sidebar_sectionTitleLink[data-active=true],
  .vocs_NavigationMenu_link[data-active=true],
  .vocs_Outline_link[data-active=true] {
    color: #e5ff1f !important;
  }
  .vocs_Content a:not(.vocs_TopNav_link):not(.vocs_Sidebar_link):not(.landing-cta) {
    color: #e5ff1f;
  }
  .vocs_Content a:not(.vocs_TopNav_link):not(.vocs_Sidebar_link):not(.landing-cta):hover {
    color: #d4f00e;
  }
  .vocs_Content code:not(pre code) {
    color: #e5ff1f;
    background: color-mix(in srgb, #e5ff1f 8%, transparent);
  }
  .vocs_Content pre code .token-string,
  .vocs_Content pre code .token.attr-value {
    color: #a3e635;
  }
  .vocs_Content pre code .token.function,
  .vocs_Content pre code .token.method {
    color: #c084fc;
  }
  .vocs_Content pre code .token.keyword,
  .vocs_Content pre code .token.boolean {
    color: #60a5fa;
  }
  .vocs_Content .landing-root {
    box-sizing: border-box;
    width: min(1120px, calc(100vw - 64px));
    max-width: none;
    margin-left: 50%;
    transform: translateX(-50%);
    padding: 32px 0 104px;
    overflow: hidden;
  }
  .vocs_Content .landing-root * {
    box-sizing: border-box;
  }
  .vocs_Content .landing-hero {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(320px, 0.9fr);
    align-items: center;
    gap: clamp(28px, 5vw, 56px);
    min-height: 520px;
    padding: 24px 0 32px;
  }
  .vocs_Content .landing-hero-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
  }
  body::before {
    content: '';
    position: fixed;
    inset: 0;
    z-index: -1;
    background:
      radial-gradient(60% 80% at 70% 50%, color-mix(in srgb, var(--vocs-color_text) 8%, transparent), transparent 70%),
      radial-gradient(40% 60% at 20% 90%, color-mix(in srgb, var(--vocs-color_text) 5%, transparent), transparent 70%);
    pointer-events: none;
  }
  .vocs_Content .landing-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 10px;
    margin-bottom: 22px;
    border: 1px solid var(--vocs-color_border);
    border-radius: 999px;
    background: color-mix(in srgb, var(--vocs-color_background) 92%, var(--vocs-color_text) 8%);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: none;
    color: var(--vocs-color_text2);
    font-variant-numeric: tabular-nums;
  }
  .vocs_Content .landing-eyebrow-dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: #e5ff1f;
    box-shadow: 0 0 0 3px color-mix(in srgb, #e5ff1f 22%, transparent);
  }
  .vocs_Content .landing-eyebrow-sep {
    color: var(--vocs-color_text3);
  }
  .vocs_Content .landing-headline {
    max-width: 720px;
    font-size: clamp(38px, 5.4vw, 64px);
    font-weight: 600;
    line-height: 1.02;
    letter-spacing: -0.03em;
    text-wrap: balance;
    color: var(--vocs-color_text);
    margin: 0 0 22px;
  }
  .vocs_Content .landing-tagline {
    font-size: clamp(15px, 1.4vw, 18px);
    line-height: 1.6;
    color: var(--vocs-color_text2);
    max-width: 650px;
    text-wrap: pretty;
    overflow-wrap: break-word;
    margin: 0 0 30px;
  }
  .vocs_Content .landing-ctas {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  .vocs_Content .landing-cta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 42px;
    padding: 10px 17px;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 600;
    text-decoration: none;
    border: 1px solid var(--vocs-color_border);
    color: var(--vocs-color_text);
    background: var(--vocs-color_background);
    transition: transform 0.18s ease, border-color 0.18s ease, background 0.18s ease, color 0.18s ease;
  }
  .vocs_Content .landing-cta:hover {
    border-color: var(--vocs-color_border2);
    color: var(--vocs-color_text);
    transform: translateY(-1px);
  }
  .vocs_Content .landing-cta:active {
    transform: translateY(0);
  }
  .vocs_Content .landing-cta-primary {
    background: #e5ff1f;
    border-color: #e5ff1f;
    color: #09090b;
  }
  .vocs_Content .landing-cta-primary:hover {
    background: #d4f00e;
    border-color: #d4f00e;
    color: #09090b;
  }
  .vocs_Content .landing-install {
    display: flex;
    align-items: stretch;
    gap: 0;
    margin-top: 22px;
    border: 1px solid var(--vocs-color_border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--vocs-color_background) 90%, var(--vocs-color_text) 10%);
    overflow: hidden;
    transition: border-color 0.18s ease;
  }
  .vocs_Content .landing-install:hover {
    border-color: var(--vocs-color_border2);
  }
  .vocs_Content .landing-install-cmd {
    flex: 1 1 auto;
    min-width: 0;
    padding: 12px 14px;
    font-family: var(--vocs-font_mono, ui-monospace, SFMono-Regular, Menlo, monospace);
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--vocs-color_text2);
    background: transparent;
    border: 0;
    white-space: nowrap;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .vocs_Content .landing-install-cmd::-webkit-scrollbar { display: none; }
  .vocs_Content .landing-install-copy {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    border: 0;
    border-left: 1px solid var(--vocs-color_border);
    background: transparent;
    color: var(--vocs-color_text2);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: background 0.18s ease, color 0.18s ease;
    font-family: inherit;
  }
  .vocs_Content .landing-install-copy:hover {
    background: color-mix(in srgb, var(--vocs-color_text) 8%, transparent);
    color: var(--vocs-color_text);
  }
  .vocs_Content .landing-install-copy:focus-visible {
    outline: 2px solid var(--vocs-color_text);
    outline-offset: -2px;
  }
  .vocs_Content .landing-install-copy.is-copied {
    color: #16a34a;
  }
  .vocs_Content .landing-trust {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    margin-top: 16px;
    font-size: 12px;
    color: var(--vocs-color_text3);
    letter-spacing: 0.04em;
  }
  .vocs_Content .landing-trust span[aria-hidden] {
    color: var(--vocs-color_border2);
  }
  .vocs_Content .landing-strip {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 1px;
    margin: 8px 0 72px;
    overflow: hidden;
    border: 1px solid var(--vocs-color_border);
    border-radius: 12px;
    background: var(--vocs-color_border);
  }
  .vocs_Content .landing-strip span {
    min-width: 0;
    padding: 14px 16px;
    text-align: center;
    font-size: 12px;
    font-weight: 600;
    color: var(--vocs-color_text2);
    background: var(--vocs-color_background);
    overflow-wrap: anywhere;
  }

  .vocs_Content .landing-section {
    margin: 78px 0 0;
  }
  .vocs_Content .landing-section-title {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--vocs-color_text3);
    margin-bottom: 24px;
  }
  .vocs_Content .landing-section-kicker {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--vocs-color_text3);
    margin-bottom: 12px;
  }
  .vocs_Content .landing-section-heading {
    max-width: 720px;
    margin-bottom: 28px;
    font-size: clamp(25px, 3vw, 36px);
    line-height: 1.12;
    letter-spacing: -0.02em;
    font-weight: 600;
    text-wrap: balance;
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
    border-radius: 10px;
    padding: 24px;
    background: color-mix(in srgb, var(--vocs-color_background) 96%, var(--vocs-color_text) 4%);
    text-align: left;
  }
  .vocs_Content .landing-feature-value {
    font-size: 34px;
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.02em;
    color: var(--vocs-color_text);
    font-variant-numeric: tabular-nums;
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
    border-radius: 10px;
    overflow: hidden;
    background: var(--vocs-color_background);
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

  .vocs_Content .landing-logo-grid {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: 22px 18px;
  }
  .vocs_Content .landing-logo {
    display: grid;
    place-items: center;
    min-height: 82px;
    margin: 0;
    padding: 4px 4px 0;
    border: none;
    border-radius: 0;
    background: transparent;
    transition: transform 0.18s ease, opacity 0.18s ease;
  }
  .vocs_Content .landing-logo:hover {
    transform: translateY(-2px);
    opacity: 0.86;
  }
  .vocs_Content .landing-logo img {
    width: 38px;
    height: 38px;
    object-fit: contain;
  }
  .vocs_Content .landing-logo figcaption {
    margin-top: 11px;
    font-size: 12px;
    font-weight: 600;
    color: var(--vocs-color_text2);
    line-height: 1.2;
    text-align: center;
  }
  .vocs_Content .landing-inline-link {
    display: inline-flex;
    margin-top: 18px;
    color: var(--vocs-color_text2);
    font-size: 14px;
    font-weight: 600;
    text-decoration: none;
  }
  .vocs_Content .landing-inline-link:hover {
    color: var(--vocs-color_text);
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

  .vocs_Content .landing-terminal {
    border: 1px solid var(--vocs-color_border);
    border-radius: 12px;
    overflow: hidden;
    background: color-mix(in srgb, var(--vocs-color_background) 94%, var(--vocs-color_text) 6%);
  }
  .vocs_Content .landing-terminal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-bottom: 1px solid var(--vocs-color_border);
    background: color-mix(in srgb, var(--vocs-color_background) 90%, var(--vocs-color_text) 10%);
  }
  .vocs_Content .landing-terminal-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--vocs-color_text2);
    font-family: var(--vocs-font_mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  }
  .vocs_Content .landing-terminal-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: #e5ff1f;
  }
  .vocs_Content .landing-terminal-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: #e5ff1f;
    box-shadow: 0 0 0 3px color-mix(in srgb, #e5ff1f 22%, transparent);
  }
  .vocs_Content .landing-terminal-body {
    margin: 0;
    padding: 20px;
    font-size: 13px;
    line-height: 1.7;
    font-family: var(--vocs-font_mono, ui-monospace, SFMono-Regular, Menlo, monospace);
    color: var(--vocs-color_text2);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .vocs_Content .landing-terminal-body::-webkit-scrollbar { display: none; }
  .vocs_Content .landing-terminal-body code {
    font-family: inherit;
    font-size: inherit;
  }
  .vocs_Content .landing-terminal-ok {
    color: #e5ff1f;
    font-weight: 600;
  }
  .vocs_Content .landing-terminal-cmd {
    color: #f97316;
    font-weight: 600;
  }
  .vocs_Content .landing-terminal-str {
    color: #a3e635;
  }
  .vocs_Content .landing-terminal-name {
    color: #c084fc;
    font-weight: 600;
  }
  .vocs_Content .landing-terminal-path {
    color: #60a5fa;
  }
  .vocs_Content .landing-terminal-cursor {
    display: inline-block;
    width: 8px;
    height: 16px;
    background: var(--vocs-color_text2);
    margin-left: 2px;
    vertical-align: text-bottom;
    animation: blink 1s step-end infinite;
  }
  @keyframes blink {
    50% { opacity: 0; }
  }

  @media (max-width: 920px) {
    .vocs_Content .landing-logo-grid {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
  }
  @media (max-width: 720px) {
    .vocs_Content .landing-root {
      width: min(100%, calc(100vw - 32px));
      padding: 20px 0 84px;
    }
    .vocs_Content .landing-headline {
      font-size: clamp(40px, 13vw, 58px);
    }
    .vocs_Content .landing-ctas {
      display: grid;
      grid-template-columns: 1fr;
    }
    .vocs_Content .landing-cta {
      width: 100%;
      justify-content: center;
    }
    .vocs_Content .landing-install {
      flex-direction: column;
    }
    .vocs_Content .landing-install-cmd {
      white-space: pre-wrap;
      word-break: break-all;
      font-size: 12px;
    }
    .vocs_Content .landing-install-copy {
      border-left: 0;
      border-top: 1px solid var(--vocs-color_border);
      padding: 9px 14px;
    }
    .vocs_Content .landing-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      margin-bottom: 58px;
    }
    .vocs_Content .landing-logo-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .vocs_Content .landing-logo {
      min-height: 100px;
    }
  }
`

const copyScript = `
  (function() {
    if (window.__lexaCopyBound) return;
    window.__lexaCopyBound = true;
    function bind() {
      document.addEventListener('click', function(e) {
        var btn = e.target.closest && e.target.closest('[data-copy]');
        if (!btn) return;
        e.preventDefault();
        var value = btn.getAttribute('data-copy');
        if (!value) return;
        var label = btn.querySelector('.landing-install-copy-label');
        var original = label ? label.textContent : 'Copy';
        function done() {
          btn.classList.add('is-copied');
          if (label) label.textContent = 'Copied';
          setTimeout(function() {
            btn.classList.remove('is-copied');
            if (label) label.textContent = original;
          }, 1500);
        }
        function fallback() {
          try {
            var ta = document.createElement('textarea');
            ta.value = value;
            ta.setAttribute('readonly', '');
            ta.style.position = 'fixed';
            ta.style.opacity = '0';
            document.body.appendChild(ta);
            ta.focus();
            ta.select();
            var ok = document.execCommand && document.execCommand('copy');
            document.body.removeChild(ta);
            if (ok) { done(); } else if (label) { label.textContent = 'Press ⌘C'; }
          } catch (_) {
            if (label) label.textContent = 'Press ⌘C';
          }
        }
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(value).then(done, fallback);
        } else {
          fallback();
        }
      });
    }
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', bind);
    } else {
      bind();
    }
  })();
`

export default await defineConfig({
  title: 'Lexa',
  description: 'Fast local code intelligence for humans and AI agents.',
  rootDir: 'src',
  logoUrl: '/brand/lexa-monochrome-dotted-flat.png',
  iconUrl: '/brand/lexa-favicon-48.png',
  ogImageUrl: null,
  // Vocs 1.4.1's `typeof config.head === 'object'` check misidentifies a
  // plain ReactElement (which is also an object) and then returns undefined
  // from the path-keyed branch. Wrapping in a function sidesteps the check.
  head: () => (
    <>
      <style>{headerCss}</style>
      <script dangerouslySetInnerHTML={{ __html: copyScript }} />
    </>
  ),
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
