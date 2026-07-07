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
					label: 'Use guide',
					translations: translations(sidebarTranslations.sections.useGuide),
					items: [
						{ label: 'Installation', translations: translations(sidebarTranslations.pages.installation), slug: 'guide/installation' },
						{ label: 'Getting started', translations: translations(sidebarTranslations.pages.gettingStarted), slug: 'guide/getting-started' },
						{ label: 'Importing', translations: translations(sidebarTranslations.pages.importing), slug: 'guide/importing' },
						{ label: 'Releases and metadata', translations: translations(sidebarTranslations.pages.releases), slug: 'guide/releases' },
						{ label: 'Browsing and search', translations: translations(sidebarTranslations.pages.browsing), slug: 'guide/browsing' },
						{ label: 'Playback', translations: translations(sidebarTranslations.pages.playback), slug: 'guide/playback' },
						{ label: 'Sync', translations: translations(sidebarTranslations.pages.sync), slug: 'guide/sync' },
						{ label: 'Devices and members', translations: translations(sidebarTranslations.pages.devices), slug: 'guide/devices' },
						{ label: 'Storage and offline', translations: translations(sidebarTranslations.pages.storage), slug: 'guide/storage' },
						{ label: 'Exporting', translations: translations(sidebarTranslations.pages.exporting), slug: 'guide/exporting' },
						{ label: 'Automation', translations: translations(sidebarTranslations.pages.automation), slug: 'guide/automation' },
					],
				},
				{
					label: 'Technical reference',
					translations: translations(sidebarTranslations.sections.technicalReference),
					items: [
						{ label: 'Architecture', translations: translations(sidebarTranslations.pages.architecture), slug: 'reference/architecture' },
						{ label: 'Data model', translations: translations(sidebarTranslations.pages.dataModel), slug: 'reference/data-model' },
						{ label: 'Sync', translations: translations(sidebarTranslations.pages.syncInternals), slug: 'reference/sync' },
						{ label: 'Cloud storage', translations: translations(sidebarTranslations.pages.cloudStorage), slug: 'reference/cloud-storage' },
						{ label: 'Encryption', translations: translations(sidebarTranslations.pages.encryption), slug: 'reference/encryption' },
						{ label: 'Identity and membership', translations: translations(sidebarTranslations.pages.membership), slug: 'reference/membership' },
						{ label: 'Import pipeline', translations: translations(sidebarTranslations.pages.importPipeline), slug: 'reference/import-pipeline' },
						{ label: 'Playback engine', translations: translations(sidebarTranslations.pages.playbackEngine), slug: 'reference/playback-engine' },
					],
				},
			],
		}),
	],
});
