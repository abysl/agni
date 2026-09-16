#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

const usage = [
  'usage: node inventory-audit.js --agni-root PATH [--label NAME] [--check] [--complete]',
  '       node inventory-audit.js PATH [--label NAME] [--check] [--complete]'
].join('\n');

function fail(message) {
  process.stderr.write(message + '\n' + usage + '\n');
  process.exitCode = 2;
}

function parseArgs(argv) {
  const options = { check: false, complete: false };
  const positional = [];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--check') {
      options.check = true;
    } else if (arg === '--complete') {
      options.complete = true;
    } else if (arg === '--agni-root' || arg === '--label' || arg === '--inventory' || arg === '--mapping') {
      if (i + 1 >= argv.length) {
        fail(arg + ' needs a value');
        return null;
      }
      options[arg.slice(2).replaceAll('-', '_')] = argv[++i];
    } else if (arg.startsWith('-')) {
      fail('unknown option: ' + arg);
      return null;
    } else {
      positional.push(arg);
    }
  }
  if (!options.agni_root && positional.length === 1) {
    options.agni_root = positional[0];
  }
  if (!options.agni_root) {
    fail('an agni root is required');
    return null;
  }
  if (positional.length > (options.agni_root === positional[0] ? 1 : 0)) {
    fail('unexpected positional argument');
    return null;
  }
  return options;
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    fail('cannot read JSON ' + file + ': ' + error.message);
    return null;
  }
}

function parseTestId(value) {
  const match = /^(.+?):(\d+) (\S+)$/.exec(value);
  if (!match) {
    throw new Error('invalid inventory test id: ' + value);
  }
  return { path: match[1], line: Number(match[2]), function: match[3] };
}

function testKey(test) {
  return test.path + '::' + test.function;
}

function bracketDelta(line) {
  let open = 0;
  let close = 0;
  let quote = false;
  let escaped = false;
  for (const character of line) {
    if (escaped) {
      escaped = false;
      continue;
    }
    if (character === '\\' && quote) {
      escaped = true;
      continue;
    }
    if (character === '"') {
      quote = !quote;
      continue;
    }
    if (quote) {
      continue;
    }
    if (character === '[') {
      open += 1;
    } else if (character === ']') {
      close += 1;
    }
  }
  return { open, close };
}

function attributeBlock(lines, functionIndex) {
  let cursor = functionIndex - 1;
  if (cursor < 0 || lines[cursor].trim() === '') {
    return { known: false, reason: 'missing contiguous test attributes' };
  }
  const collected = [];
  let reverseBalance = 0;
  while (cursor >= 0) {
    const line = lines[cursor];
    const trimmed = line.trim();
    if (trimmed === '') {
      const previous = cursor > 0 ? lines[cursor - 1].trim() : '';
      if (previous.startsWith('#[')) {
        return { known: false, reason: 'blank-separated attributes' };
      }
      break;
    }
    const delta = bracketDelta(line);
    reverseBalance += delta.close - delta.open;
    collected.unshift(trimmed);
    if (reverseBalance < 0) {
      return { known: false, reason: 'malformed attribute brackets' };
    }
    cursor -= 1;
    if (reverseBalance > 0) {
      continue;
    }
    if (cursor >= 0 && lines[cursor].trim().startsWith('#[')) {
      continue;
    }
    if (cursor >= 0 && lines[cursor].trim() === '') {
      const previous = cursor > 0 ? lines[cursor - 1].trim() : '';
      if (previous.startsWith('#[')) {
        return { known: false, reason: 'blank-separated attributes' };
      }
    }
    if (cursor >= 0 && !lines[cursor].trim().startsWith('#[')) {
      const previousLine = bracketDelta(lines[cursor]);
      if (previousLine.close > previousLine.open) {
        return { known: false, reason: 'multiline attribute continuation' };
      }
    }
    break;
  }
  if (reverseBalance !== 0 || collected.some((line) => !line.startsWith('#[') || bracketDelta(line).open !== bracketDelta(line).close)) {
    return { known: false, reason: 'multiline or malformed attributes' };
  }
  const names = collected.map((line) => /^#\[\s*([A-Za-z_][A-Za-z0-9_]*)/.exec(line)?.[1] || null);
  if (names.some((name) => !['test', 'ignore', 'should_panic'].includes(name))) {
    return { known: false, reason: 'unsupported test attribute metadata' };
  }
  const testAttribute = collected.find((line) => /^#\[\s*test(?:\s*\([^)]*\))?\s*\]$/.test(line));
  if (!testAttribute) {
    return { known: false, reason: 'function has no unambiguous test attribute' };
  }
  const ignoreAttributes = collected.filter((line) => /^#\[\s*ignore(?:\s*=.*)?\s*\]$/.test(line));
  if (ignoreAttributes.length > 1) {
    return { known: false, reason: 'multiple ignore attributes' };
  }
  let ignoreReason = null;
  if (ignoreAttributes.length === 1) {
    const ignore = ignoreAttributes[0];
    if (ignore === '#[ignore]') {
      ignoreReason = 'ignore';
    } else {
      const reason = /^#\[\s*ignore\s*=\s*"((?:\\.|[^"])*)"\s*\]$/.exec(ignore);
      if (!reason) {
        return { known: false, reason: 'ignore attribute is not understood' };
      }
      ignoreReason = reason[1];
    }
  }
  return { known: true, attributes: collected, ignoreReason };
}

function indexSource(root, relativePath) {
  const file = path.join(root, relativePath);
  if (!fs.existsSync(file)) {
    return { state: 'absent', path: relativePath };
  }
  const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
  const functions = new Map();
  for (let index = 0; index < lines.length; index += 1) {
    const match = /\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^\n]*>)?\s*\(/.exec(lines[index]);
    if (!match) {
      continue;
    }
    const name = match[1];
    if (functions.has(name)) {
      functions.set(name, { state: 'ambiguous', path: relativePath, function: name, reason: 'duplicate function name' });
      continue;
    }
    const attributes = attributeBlock(lines, index);
    if (!attributes.known) {
      functions.set(name, {
        state: 'unknown',
        path: relativePath,
        function: name,
        line: index + 1,
        reason: attributes.reason
      });
      continue;
    }
    functions.set(name, {
      state: 'known',
      path: relativePath,
      function: name,
      line: index + 1,
      attributes: attributes.attributes,
      ignoreReason: attributes.ignoreReason
    });
  }
  return { state: 'source', path: relativePath, functions };
}

function classify(source, mapping, resolvedMapping) {
  if (!source || source.state === 'absent') {
    if (mapping && resolvedMapping && resolvedMapping.enabled === resolvedMapping.total) {
      return mapping.classification === 'rules_question'
        ? 'replaced_rules_question'
        : 'replaced_engine_gap';
    }
    return 'absent_unmapped';
  }
  if (source.state !== 'known') {
    return 'unknown';
  }
  if (source.ignoreReason === null) {
    return 'enabled';
  }
  const reason = source.ignoreReason.toLowerCase();
  if (reason.startsWith('rules question')) {
    return mapping ? 'ignored_rules_question_with_replacement' : 'unknown';
  }
  if (reason.startsWith('engine gap')) {
    return 'ignored_engine_gap';
  }
  return 'unknown';
}

function resolveMapping(mapping, root) {
  if (!mapping) {
    return null;
  }
  const replacements = mapping.replacements.map((replacement) => {
    const indexed = indexSource(root, replacement.path);
    const source = indexed.state === 'source' && indexed.functions
      ? indexed.functions.get(replacement.function)
      : null;
    const known = Boolean(source && source.state === 'known');
    return {
      path: replacement.path,
      function: replacement.function,
      state: source ? source.state : 'absent',
      present: known,
      line: known ? source.line : null,
      ignore_reason: known ? source.ignoreReason : null,
      enabled: Boolean(known && source.ignoreReason === null)
    };
  });
  return {
    classification: mapping.classification,
    reason: mapping.reason,
    expected_labels: mapping.expected_labels || [],
    replacements,
    found: replacements.filter((replacement) => replacement.present).length,
    enabled: replacements.filter((replacement) => replacement.enabled).length,
    total: replacements.length
  };
}

function mappingIsComplete(resolvedMapping) {
  return Boolean(
    resolvedMapping
      && resolvedMapping.total > 0
      && resolvedMapping.enabled === resolvedMapping.total
  );
}

function completionReasons(source, status, mapping, resolvedMapping) {
  const reasons = [];
  if (status === 'ignored_engine_gap' && !mappingIsComplete(resolvedMapping)) {
    reasons.push('unresolved engine-gap ignore');
  }
  if (status === 'unknown') {
    reasons.push(source.reason || 'unknown or conditional source shape');
  }
  if (status === 'absent_unmapped') {
    reasons.push('absent or unmapped original');
  }
  if (mapping && !mappingIsComplete(resolvedMapping)) {
    if (!resolvedMapping || resolvedMapping.total === 0) {
      reasons.push('replacement mapping has no replacements');
    } else {
      reasons.push(
        `replacement mapping incomplete (${resolvedMapping.enabled}/${resolvedMapping.total} enabled)`
      );
    }
  }
  if (status === 'ignored_rules_question_with_replacement' && !mappingIsComplete(resolvedMapping)) {
    reasons.push('rules-question replacement is incomplete');
  }
  return [...new Set(reasons)];
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (!options) {
    return;
  }
  const scriptDirectory = __dirname;
  const defaultInventory = path.join(scriptDirectory, 'inventory.json');
  const defaultMapping = path.join(scriptDirectory, 'inventory-audit-mappings.json');
  const inventoryPath = options.inventory || defaultInventory;
  const mappingPath = options.mapping || defaultMapping;
  const inventory = readJson(inventoryPath);
  const mappingDocument = readJson(mappingPath);
  if (!inventory || !mappingDocument) {
    return;
  }
  if (!Array.isArray(inventory.clusters) || !Array.isArray(mappingDocument.replacements)) {
    fail('inventory or mapping schema is invalid');
    return;
  }

  const mappings = new Map();
  for (const mapping of mappingDocument.replacements) {
    const key = testKey(mapping.original);
    if (mappings.has(key)) {
      fail('duplicate mapping for ' + key);
      return;
    }
    mappings.set(key, mapping);
  }

  const sourceCache = new Map();
  const sourceFor = (test) => {
    if (!sourceCache.has(test.path)) {
      sourceCache.set(test.path, indexSource(options.agni_root, test.path));
    }
    const indexed = sourceCache.get(test.path);
    if (indexed.state !== 'source' || !indexed.functions) {
      return indexed;
    }
    return indexed.functions.get(test.function) || { state: 'absent', path: test.path, function: test.function };
  };

  const entries = [];
  const counts = {};
  const missing = [];
  const unexplainedMissing = [];
  const mappingSummary = [];
  const unresolved = [];
  const unresolvedReasons = {};
  for (const cluster of inventory.clusters) {
    for (const rawTest of cluster.tests) {
      let test;
      try {
        test = parseTestId(rawTest);
      } catch (error) {
        fail(error.message);
        return;
      }
      const mapping = mappings.get(testKey(test)) || null;
      const resolvedMapping = resolveMapping(mapping, options.agni_root);
      const source = sourceFor(test);
      const status = classify(source, mapping, resolvedMapping);
      counts[status] = (counts[status] || 0) + 1;
      const entry = {
        cluster: {
          key: cluster.key,
          title: cluster.title,
          lane: cluster.lane,
          order: cluster.order
        },
        original: rawTest,
        source: source.state === 'known'
          ? {
              path: source.path,
              function: source.function,
              line: source.line,
              requested_line: test.line,
              ignore_reason: source.ignoreReason
            }
          : {
              path: test.path,
              function: test.function,
              requested_line: test.line,
              state: source.state,
              reason: source.reason || null
            },
        status
      };
      if (resolvedMapping) {
        entry.mapping = resolvedMapping;
      }
      entries.push(entry);
      if (status === 'absent_unmapped') {
        missing.push(rawTest);
      }
      const mappingApplies = resolvedMapping
        && options.label
        && resolvedMapping.expected_labels.includes(options.label);
      const missingMappedReplacement = source.state === 'absent'
        && mappingApplies
        && resolvedMapping.found === 0;
      const unavailableMappedReplacement = mappingApplies
        && resolvedMapping.enabled !== resolvedMapping.total;
      if (status === 'absent_unmapped' || missingMappedReplacement || unavailableMappedReplacement) {
        unexplainedMissing.push(rawTest);
        entry.audit = { unexplained_missing: true };
      }
      if (options.complete) {
        const reasons = completionReasons(source, status, mapping, resolvedMapping);
        if (reasons.length > 0) {
          entry.audit = { ...(entry.audit || {}), complete: false, reasons };
          unresolved.push({ original: rawTest, status, reasons });
          for (const reason of reasons) {
            unresolvedReasons[reason] = (unresolvedReasons[reason] || 0) + 1;
          }
        }
      }
    }
  }

  for (const [key, mapping] of mappings) {
    const found = entries.find((entry) => testKey(parseTestId(entry.original)) === key);
    if (!found) {
      fail('mapping original is not in the immutable inventory: ' + key);
      return;
    }
    const resolved = found.mapping;
    mappingSummary.push({
      original: key,
      classification: mapping.classification,
      expected_labels: mapping.expected_labels || [],
      found: resolved.found,
      enabled: resolved.enabled,
      total: resolved.total,
      all_found: resolved.found === resolved.total,
      all_enabled: resolved.enabled === resolved.total,
      check_applicable: Boolean(options.label && (mapping.expected_labels || []).includes(options.label)),
      check_passed: !options.label
        || !(mapping.expected_labels || []).includes(options.label)
        || resolved.enabled === resolved.total,
      complete_passed: mappingIsComplete(resolved)
    });
  }

  const clusters = inventory.clusters.map((cluster) => {
    const clusterEntries = entries.filter((entry) => entry.cluster.key === cluster.key);
    const clusterCounts = {};
    for (const entry of clusterEntries) {
      clusterCounts[entry.status] = (clusterCounts[entry.status] || 0) + 1;
    }
    return {
      key: cluster.key,
      title: cluster.title,
      lane: cluster.lane,
      order: cluster.order,
      entries: clusterEntries.length,
      counts: clusterCounts
    };
  });

  const output = {
    schema: 1,
    label: options.label || null,
    complete: options.complete,
    agni_root: options.agni_root,
    inventory: path.relative(options.agni_root, inventoryPath),
    mapping: path.relative(options.agni_root, mappingPath),
    summary: {
      clusters: inventory.clusters.length,
      entries: entries.length,
      counts,
      absent_unmapped: missing.length,
      unexplained_missing: unexplainedMissing.length,
      complete_unresolved: unresolved.length,
      unresolved_reasons: unresolvedReasons,
      mappings: mappingSummary
    },
    clusters,
    entries,
    unresolved
  };
  process.stdout.write(JSON.stringify(output, null, 2) + '\n');
  if (options.check && unexplainedMissing.length > 0) {
    process.exitCode = 1;
  }
  if (options.complete && unresolved.length > 0) {
    process.exitCode = 1;
  }
}

main();
