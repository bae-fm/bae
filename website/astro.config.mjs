// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { existsSync, readFileSync } from 'node:fs';
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

const docsRoot = new URL('./src/content/docs/', import.meta.url);

function readDocTitle(locale, slug, fallback) {
	const prefix = locale.path ? `${locale.path}/` : '';
	const file = new URL(`${prefix}${slug}.mdx`, docsRoot);
	if (!existsSync(file)) return fallback;

	const source = readFileSync(file, 'utf8');
	const frontmatter = source.match(/^---\n([\s\S]*?)\n---/);
	const title = frontmatter?.[1].match(/^title:\s*(.+)$/m)?.[1]?.trim();
	return title?.replace(/^["']|["']$/g, '') ?? fallback;
}

function docTitleTranslations(slug, fallback) {
	return Object.fromEntries(
		LOCALES.map((locale) => [locale.lang, readDocTitle(locale, slug, fallback)])
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
						{ label: 'Installation', translations: docTitleTranslations('guide/installation', 'Installation'), slug: 'guide/installation' },
						{ label: 'Getting started', translations: docTitleTranslations('guide/getting-started', 'Getting started'), slug: 'guide/getting-started' },
						{ label: 'Importing', translations: docTitleTranslations('guide/importing', 'Importing'), slug: 'guide/importing' },
						{ label: 'Releases and metadata', translations: docTitleTranslations('guide/releases', 'Releases and metadata'), slug: 'guide/releases' },
						{ label: 'Browsing and search', translations: docTitleTranslations('guide/browsing', 'Browsing and search'), slug: 'guide/browsing' },
						{ label: 'Playback', translations: docTitleTranslations('guide/playback', 'Playback'), slug: 'guide/playback' },
						{ label: 'Sync', translations: docTitleTranslations('guide/sync', 'Sync'), slug: 'guide/sync' },
						{ label: 'Devices and members', translations: docTitleTranslations('guide/devices', 'Devices and members'), slug: 'guide/devices' },
						{ label: 'Storage and offline', translations: docTitleTranslations('guide/storage', 'Storage and offline'), slug: 'guide/storage' },
						{ label: 'Exporting', translations: docTitleTranslations('guide/exporting', 'Exporting'), slug: 'guide/exporting' },
						{ label: 'Automation', translations: docTitleTranslations('guide/automation', 'Automation'), slug: 'guide/automation' },
					],
				},
				{
					label: 'Technical reference',
					translations: translations(sidebarTranslations.sections.technicalReference),
					items: [
						{ label: 'Architecture', translations: docTitleTranslations('reference/architecture', 'Architecture'), slug: 'reference/architecture' },
						{ label: 'Data model', translations: docTitleTranslations('reference/data-model', 'Data model'), slug: 'reference/data-model' },
						{ label: 'Sync', translations: docTitleTranslations('reference/sync', 'Sync'), slug: 'reference/sync' },
						{ label: 'Cloud storage', translations: docTitleTranslations('reference/cloud-storage', 'Cloud storage'), slug: 'reference/cloud-storage' },
						{ label: 'Encryption', translations: docTitleTranslations('reference/encryption', 'Encryption'), slug: 'reference/encryption' },
						{ label: 'Identity and membership', translations: docTitleTranslations('reference/membership', 'Identity and membership'), slug: 'reference/membership' },
						{ label: 'Import pipeline', translations: docTitleTranslations('reference/import-pipeline', 'Import pipeline'), slug: 'reference/import-pipeline' },
						{ label: 'Playback engine', translations: docTitleTranslations('reference/playback-engine', 'Playback engine'), slug: 'reference/playback-engine' },
					],
				},
			],
		}),
	],
});
