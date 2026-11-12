import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default defineConfig(
  { ignores: ['build/', '.svelte-kit/', 'node_modules/', 'src/lib/api/schema.gen.ts'] },
  js.configs.recommended,
  ts.configs.strictTypeChecked,
  ts.configs.stylisticTypeChecked,
  svelte.configs.recommended,
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser } },
  },
  {
    // `{@render snippet()}` in markup is a void call by design, not a confusing expression.
    files: ['**/*.svelte'],
    rules: { '@typescript-eslint/no-confusing-void-expression': 'off' },
  },
  {
    files: ['**/*.js'],
    extends: [ts.configs.disableTypeChecked],
  },
  // Layers of the web app (architecture.md §14): foundations ← ui ← shell, features ← routes.
  {
    files: [
      'src/lib/{api,i18n,live,session,store,theme,time,url}/**/*.{ts,svelte}',
      'src/lib/storage.ts',
    ],
    ignores: ['**/*.test.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/ui/**', '**/shell/**', '**/features/**'],
              message:
                'Foundations must not depend on UI, the shell or features (architecture §14).',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/lib/ui/**/*.{ts,svelte}'],
    ignores: ['**/*.test.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/shell/**', '**/features/**', '$app/*'],
              message:
                'lib/ui is the design system: no shell, feature or SvelteKit code (architecture §14).',
            },
          ],
        },
      ],
    },
  },
  {
    files: ['src/lib/shell/**/*.{ts,svelte}', 'src/lib/features/**/*.{ts,svelte}'],
    ignores: ['**/*.test.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['**/routes/**'],
              message:
                'Routes assemble pages from lib; lib never imports routes (architecture §14).',
            },
          ],
        },
      ],
    },
  },
);
