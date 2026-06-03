import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import lucode from 'lucode-starlight';
import remarkGfm from 'remark-gfm';

export default defineConfig({
  markdown: {
    remarkPlugins: [remarkGfm],
  },
  integrations: [
    starlight({
      title: 'Lexa',
      description: 'Fast local code intelligence for humans and AI agents.',
      logo: {
        src: './src/assets/anvia-logo.png',
        alt: 'Lexa',
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/anvia-hq/lexa' },
      ],
      editLink: {
        baseUrl: 'https://github.com/anvia-hq/lexa/edit/main/www/src/content/docs/',
      },
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Overview', slug: 'index' },
            { label: 'Install', slug: 'guides/install' },
            { label: 'Quick Start', slug: 'guides/quick-start' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Commands', slug: 'reference/commands' },
            { label: 'MCP Server', slug: 'reference/mcp' },
          ],
        },
      ],
      plugins: [
        lucode({
          navLinks: [
            { label: 'Docs', link: '/guides/quick-start/' },
            { label: 'Commands', link: '/reference/commands/' },
          ],
          footerText:
            'Built with [Astro Starlight](https://starlight.astro.build/) and [Lucode Starlight](https://github.com/lucas-labs/lucode-starlight-theme).',
        }),
      ],
    }),
  ],
});
