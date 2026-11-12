// Links between issues (LNK): the link types of the instance and their administration page. Other
// features use link types only through this file.
export * as linkMessages from './messages.ts';
export { default as LinkTypesPage } from './LinkTypesPage.svelte';
export { DIRECTIONS, ensureLinkTypes, linkTypeRow } from './link-types.ts';
