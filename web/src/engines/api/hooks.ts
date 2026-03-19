/**
 * Platform hooks re-exported for engine consumption.
 *
 * Engines import from '@/engines/api' rather than reaching into
 * feature barrels directly. This keeps the dependency explicit and
 * gives us a single place to rewire when rooms moves fully into engines.
 */

export { useRoom } from '@/features/rooms';
