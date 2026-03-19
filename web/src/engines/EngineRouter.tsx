import React, { Suspense } from 'react';
import { Center, Loader } from '@mantine/core';
import type { EngineViewProps } from './api';
import { engineMap } from './registry';

export function EngineRouter(props: EngineViewProps) {
  const loader = engineMap[props.room.engine_type];
  if (!loader) {
    return <Center>Unknown room type: {props.room.engine_type}</Center>;
  }
  const LazyEngine = React.lazy(async () => {
    const mod = await loader();
    return { default: mod.EngineView };
  });
  return (
    <Suspense
      fallback={
        <Center>
          <Loader />
        </Center>
      }
    >
      <LazyEngine {...props} />
    </Suspense>
  );
}
