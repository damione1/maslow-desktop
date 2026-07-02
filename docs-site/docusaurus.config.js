// @ts-check
// `@type` JSDoc annotations allow editor autocompletion and type checking
// (when paired with `@ts-check`).
// See: https://docusaurus.io/docs/api/docusaurus-config

import {themes as prismThemes} from 'prism-react-renderer';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Maslow Desktop',
  tagline: 'A friendly control panel for the Maslow CNC, and its HTTP/gRPC/MCP control API',
  favicon: 'img/favicon.ico',

  future: {
    v4: true, // Improve compatibility with the upcoming Docusaurus v4
  },

  // Placeholder until the GitHub Pages deploy pipeline lands (a later PR);
  // this PR only needs `npm run build` to produce a working static site.
  url: 'https://damione1.github.io',
  baseUrl: '/maslow-desktop/',

  organizationName: 'damione1',
  projectName: 'maslow-desktop',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  // Generated API reference content is full of proto/Rust syntax like
  // `map<string, string>` or `Vec<String>` in plain prose, which the default
  // MDX (JSX-aware) parser misreads as broken JSX tags. 'detect' parses every
  // `.md` file (all of them, in this site) as plain CommonMark instead;
  // nothing here needs JSX/MDX component features.
  markdown: {
    format: 'detect',
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/damione1/maslow-desktop/tree/master/docs-site/',
        },
        // No blog: this site is API/app reference documentation, not a
        // project blog.
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: 'img/docusaurus-social-card.jpg',
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'Maslow Desktop',
        logo: {
          alt: 'Maslow Desktop logo',
          src: 'img/logo.svg',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Docs',
          },
          {
            href: 'https://github.com/damione1/maslow-desktop',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {label: 'App overview', to: '/docs/intro'},
              {label: 'Using the API', to: '/docs/api/using-the-api'},
            ],
          },
          {
            title: 'More',
            items: [
              {label: 'GitHub', href: 'https://github.com/damione1/maslow-desktop'},
              {label: 'Releases', href: 'https://github.com/damione1/maslow-desktop/releases'},
            ],
          },
        ],
        copyright: `Maslow Desktop is a community project, not affiliated with Maslow CNC or FluidNC. Docs built with Docusaurus.`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
      },
    }),
};

export default config;
