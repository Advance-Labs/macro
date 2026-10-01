import type { deriveRuntimeCacheSchema } from '../../../../../scripts/generate-graphql-cache-runtime-schema';
import { runtimeCacheSchema } from '../../runtime-schema';
import init, { openCache } from '../../wasm/cache_wasm';

type Metadata = ReturnType<typeof deriveRuntimeCacheSchema>;

async function run() {
  await init();
  const scope = `runtime-schema-${crypto.randomUUID()}`;
  let engine = await openCache(scope);
  const schema: Metadata = JSON.parse(runtimeCacheSchema);
  const messageType = schema.types.find(
    (type) => type.name === 'GraphqlSoupEmailMessage'
  )!;
  messageType.fields.push({
    name: 'otaCalendarInvitations',
    ty: {
      name: 'JSON',
      kind: 'OpaqueScalar',
      nullable: false,
      list: false,
      item_nullable: false,
    },
  });
  const narrow =
    'query { user { id emailThread(input: { threadId: "thread" }) { id messages(offset: 0, limit: 1) { id bodyText } } } }';
  const full = narrow.replace(
    'id bodyText',
    'id bodyText otaCalendarInvitations'
  );
  const data = {
    user: {
      id: 'viewer',
      emailThread: {
        id: 'thread',
        messages: [
          {
            id: 'message',
            bodyText: 'cached body',
            otaCalendarInvitations: [{ uid: 'meeting' }],
          },
        ],
      },
    },
  };
  try {
    await engine.configureSchema(runtimeCacheSchema);
    await engine.writeQuery({}, narrow, undefined, {}, data, 'viewer');
    const generation = await engine.currentStorageGeneration();
    let rejectedBeforeUpdate = false;
    try {
      await engine.writeQuery({}, full, undefined, {}, data, 'viewer');
    } catch {
      rejectedBeforeUpdate = true;
    }
    await engine.configureSchema(JSON.stringify(schema));
    const beforeFetch = await engine.readQuery(
      undefined,
      full,
      undefined,
      {},
      undefined
    );
    await engine.writeQuery({}, full, undefined, {}, data, 'viewer');
    await engine.writeQuery({}, narrow, undefined, {}, data, 'viewer');
    await engine.close();
    engine = await openCache(scope);
    await engine.configureSchema(runtimeCacheSchema);
    const reopened = await engine.readQuery(
      undefined,
      full,
      undefined,
      {},
      undefined
    );
    const afterGeneration = await engine.currentStorageGeneration();
    messageType.fields.find(
      (field) => field.name === 'otaCalendarInvitations'
    )!.ty.name = 'String';
    let incompatibleRejected = false;
    try {
      await engine.configureSchema(JSON.stringify(schema));
    } catch {
      incompatibleRejected = true;
    }
    const afterRejection = await engine.readQuery(
      undefined,
      full,
      undefined,
      {},
      undefined
    );
    await engine.physicalReset();
    await engine.writeQuery({}, full, undefined, {}, data, 'viewer');
    const afterReset = await engine.readQuery(
      undefined,
      full,
      undefined,
      {},
      undefined
    );
    return {
      afterReset,
      rejectedBeforeUpdate,
      beforeFetch,
      reopened,
      incompatibleRejected,
      afterRejection,
      sameGeneration: generation === afterGeneration,
    };
  } finally {
    await engine.close();
  }
}

self.onmessage = async () => {
  try {
    self.postMessage({ result: await run() });
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
