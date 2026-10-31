// The addresses of providers, connectors and local model servers, from the one file both sides read
// (endpoints.json; Rust reads it in src-tauri/src/endpoints.rs). Change an address there, not where it is used.
import endpoints from './endpoints.json';

export type LocalEndpoint = { kind: 'openai' | 'anthropic' | 'ollama'; label: string; base: string };
export type CloudEndpoint = { name: string; base: string; keyEnv: string; model: string; note: string; keyUrl: string; restricted?: boolean; capabilities?: string[] };

export const ENDPOINTS = endpoints as Omit<typeof endpoints, 'local' | 'freeCloud' | 'featuredCloud'> & {
  local: LocalEndpoint[];
  freeCloud: CloudEndpoint[];
  featuredCloud: CloudEndpoint[];
};
