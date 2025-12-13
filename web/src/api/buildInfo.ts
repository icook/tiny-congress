import * as z from 'zod';
import { BuildInfoSchema, type BuildInfo } from './generated/graphql';
import { graphqlRequest } from './graphqlClient';

const BUILD_INFO_QUERY = `
  query BuildInfoQuery {
    buildInfo {
      version
      gitSha
      buildTime
      message
    }
  }
`;

// Query result wrapper schema
const buildInfoQueryResultSchema = z.object({
  buildInfo: BuildInfoSchema,
});

type BuildInfoQueryResult = z.infer<typeof buildInfoQueryResultSchema>;

export async function fetchBuildInfo(): Promise<BuildInfo> {
  const data = await graphqlRequest<BuildInfoQueryResult>(BUILD_INFO_QUERY);

  // Validate response at runtime
  const result = buildInfoQueryResultSchema.parse(data);

  return result.buildInfo;
}

export type { BuildInfo };
