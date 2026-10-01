import metadata from './generated/runtime-schema.json?raw';

/** Shipped in the frontend/OTA bundle, independent of the native build. */
export const runtimeCacheSchema = metadata;

export type CacheSchemaAcknowledgement = {
  protocolVersion: number;
  fingerprint: string;
};

/** A void response from an older engine is not a successful negotiation. */
export function assertCacheSchemaAcknowledged(
  value: unknown
): asserts value is CacheSchemaAcknowledgement {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('protocolVersion' in value) ||
    value.protocolVersion !== 1 ||
    !('fingerprint' in value) ||
    typeof value.fingerprint !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.fingerprint)
  ) {
    throw new Error(
      'This cache engine cannot load the app schema. Update the app to enable offline storage.'
    );
  }
}
