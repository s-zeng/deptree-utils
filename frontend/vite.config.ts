import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';

export default defineConfig({
  plugins: [
    viteSingleFile({
      useRecommendedBuildConfig: true,
      removeViteModuleLoader: true,
    }),
  ],
  build: {
    target: 'es2020',
    assetsInlineLimit: 100000000, // Inline everything (WASM, CSS, JS)
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
});
