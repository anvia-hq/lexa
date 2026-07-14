export function parseArguments(argv, allowed) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (!allowed.has(key)) {
      throw new Error(`unknown argument: ${key}`);
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`missing value for ${key}`);
    }
    parsed[key.slice(2)] = value;
    index += 1;
  }
  return parsed;
}

export function requiredArgument(parsed, name) {
  const value = parsed[name];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`missing required argument --${name}`);
  }
  return value;
}
