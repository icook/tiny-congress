import { graphqlRequest } from './graphqlClient';

export type BuildInfo = {
  version: string;
  gitSha: string;
  buildTime: string;
  message?: string | null;
};

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

type BuildInfoQueryResult = {
  buildInfo: BuildInfo;
};

export async function fetchBuildInfo(): Promise<BuildInfo> {
  const data = await graphqlRequest<BuildInfoQueryResult>(BUILD_INFO_QUERY);
  return data.buildInfo;
}
