import { graphqlRequest } from './graphqlClient';
import { buildInfoQueryResultSchema, type BuildInfo, type BuildInfoQueryResult } from './schemas';

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

export async function fetchBuildInfo(): Promise<BuildInfo> {
  const data = await graphqlRequest<BuildInfoQueryResult>(BUILD_INFO_QUERY);

  // Validate response at runtime
  const result = buildInfoQueryResultSchema.parse(data);

  return result.buildInfo;
}

export type { BuildInfo };
