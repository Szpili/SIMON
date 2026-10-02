import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Repo name is the base path on GitHub Pages. Override with BASE_PATH= (empty)
// to build a bundle that opens straight from the filesystem.
const dev = process.argv.includes('dev');
const base = dev ? '' : (process.env.BASE_PATH ?? '/SIMON');

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ pages: 'build', assets: 'build', fallback: undefined, strict: true }),
    paths: { base, relative: true }
  }
};
