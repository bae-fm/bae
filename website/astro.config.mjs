// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://bae.fm',
	integrations: [
		starlight({
			title: 'bae',
			description: 'Music library manager with serverless, encrypted, multi-device sync',
			favicon: '/app-icon.png',
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/bae-fm/bae' }
			],
			customCss: ['./src/styles/custom.css'],
			// Fenced code blocks render through Expressive Code, which has its
			// own theming and ignores custom.css. Pin its surface to the Direction
			// D night-violet palette so code blocks match the rest of the docs.
			// JetBrains Mono comes in already, via --sl-font-mono.
			expressiveCode: {
				styleOverrides: {
					borderRadius: '12px',
					borderColor: 'rgba(253, 245, 239, 0.12)',
					codeBackground: '#161327',
					frames: {
						editorActiveTabBackground: '#1c1830',
						editorTabBarBackground: '#100e1a',
						editorActiveTabIndicatorBottomColor: '#9b6cf6',
						terminalTitlebarBackground: '#1c1830',
						terminalBackground: '#161327',
					},
				},
			},
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quick Start', slug: 'getting-started/quick-start' },
					],
				},
				{
					label: 'Library',
					items: [
						{ label: 'Importing', slug: 'importing/local-files' },
						{ label: 'Metadata', slug: 'library/metadata' },
						{ label: 'Browsing', slug: 'library/browsing' },
					],
				},
				{
					label: 'Storage',
					items: [
						{ label: 'Overview', slug: 'storage/overview' },
						{ label: 'Sync', slug: 'storage/sync' },
					],
				},
				{
					label: 'Architecture',
					items: [
						{ label: 'Overview', slug: 'architecture/overview' },
						{ label: 'Data Model', slug: 'architecture/data-model' },
						{ label: 'Cloud Home', slug: 'architecture/cloud-home' },
						{ label: 'Encryption', slug: 'architecture/encryption' },
						{ label: 'Membership', slug: 'architecture/membership' },
						{ label: 'Serverless', slug: 'architecture/serverless' },
					],
				},
			],
		}),
	],
});
