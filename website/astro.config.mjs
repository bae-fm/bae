// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { LOCALES, NON_DEFAULT_LOCALES, sidebarTranslations } from './src/lib/site-locales.mjs';

const localeConfig = {
	root: {
		label: LOCALES.find((locale) => locale.code === 'en').label,
		lang: 'en',
	},
	...Object.fromEntries(
		NON_DEFAULT_LOCALES.map((locale) => [
			locale.path,
			{
				label: locale.label,
				lang: locale.lang,
				dir: locale.dir,
			},
		])
	),
};

function translations(group) {
	return Object.fromEntries(
		LOCALES.map((locale) => [locale.lang, group[locale.code] ?? group.en])
	);
}

// https://astro.build/config
export default defineConfig({
	site: 'https://bae.fm',
	integrations: [
		starlight({
			title: 'bae',
			description: 'Music library manager with serverless, encrypted, multi-device sync',
			favicon: '/app-icon.png',
			locales: localeConfig,
			defaultLocale: 'root',
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
					translations: translations(sidebarTranslations.sections.gettingStarted),
					items: [
						{ label: 'Installation', translations: translations(sidebarTranslations.pages.installation), slug: 'getting-started/installation' },
						{ label: 'Quick Start', translations: translations(sidebarTranslations.pages.quickStart), slug: 'getting-started/quick-start' },
					],
				},
				{
					label: 'Library',
					translations: translations(sidebarTranslations.sections.library),
					items: [
						{ label: 'Importing', translations: translations(sidebarTranslations.pages.importing), slug: 'importing/local-files' },
						{ label: 'Metadata', translations: translations(sidebarTranslations.pages.metadata), slug: 'library/metadata' },
						{ label: 'Browsing', translations: translations(sidebarTranslations.pages.browsing), slug: 'library/browsing' },
					],
				},
				{
					label: 'Storage',
					translations: translations(sidebarTranslations.sections.storage),
					items: [
						{ label: 'Overview', translations: translations(sidebarTranslations.pages.overview), slug: 'storage/overview' },
						{ label: 'Sync', translations: translations(sidebarTranslations.pages.sync), slug: 'storage/sync' },
					],
				},
				{
					label: 'Architecture',
					translations: translations(sidebarTranslations.sections.architecture),
					items: [
						{ label: 'Overview', translations: translations(sidebarTranslations.pages.overview), slug: 'architecture/overview' },
						{ label: 'Data Model', translations: translations(sidebarTranslations.pages.dataModel), slug: 'architecture/data-model' },
						{ label: 'Cloud Home', translations: translations(sidebarTranslations.pages.cloudHome), slug: 'architecture/cloud-home' },
						{ label: 'Encryption', translations: translations(sidebarTranslations.pages.encryption), slug: 'architecture/encryption' },
						{ label: 'Membership', translations: translations(sidebarTranslations.pages.membership), slug: 'architecture/membership' },
						{ label: 'Serverless', translations: translations(sidebarTranslations.pages.serverless), slug: 'architecture/serverless' },
					],
				},
			],
		}),
	],
});
