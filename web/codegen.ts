import type { CodegenConfig } from '@graphql-codegen/cli';

const config: CodegenConfig = {
  schema: './schema.graphql',
  generates: {
    // Generate both types and schemas in one file to avoid import mismatches
    './src/api/generated/graphql.ts': {
      plugins: ['typescript', 'graphql-codegen-typescript-validation-schema'],
      config: {
        // TypeScript plugin config
        scalars: {
          ID: 'string',
        },
        // Use 'type' instead of 'interface' for consistency
        declarationKind: 'type',
        // Avoid enum objects, use string unions
        enumsAsTypes: true,
        // Make nullable fields optional
        avoidOptionals: false,
        // Skip __typename in output
        skipTypename: true,

        // Validation schema plugin config (zodv4 for Zod v4 support)
        schema: 'zodv4',
        validationSchemaExportType: 'const',
        scalarSchemas: {
          ID: 'z.string()',
        },
        // Generate schemas for all object types
        withObjectType: true,
      },
    },
  },
  hooks: {
    afterAllFileWrite: ['prettier --write'],
  },
};

export default config;
