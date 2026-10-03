import { FileTypeMap } from '@service-storage/fileTypeMap';

// Extension metadata shared by file classification and the code editor.
export const extensionToLanguage: Record<string, string> = {
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'tsx',
  cts: 'typescript',
  mts: 'typescript',
  html: 'html',
  htm: 'html',
  xhtml: 'html',
  shtml: 'html',
  css: 'css',
  scss: 'css',
  sass: 'css',
  less: 'css',
  json: 'json',
  jsonc: 'json',
  py: 'python',
  pyw: 'python',
  pyi: 'python',
  rpy: 'python',
  rs: 'rust',
  c: 'c',
  h: 'c',
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  'c++': 'cpp',
  hpp: 'cpp',
  hh: 'cpp',
  hxx: 'cpp',
  'h++': 'cpp',
  ii: 'cpp',
  ino: 'cpp',
  inl: 'cpp',
  ipp: 'cpp',
  ixx: 'cpp',
  cppm: 'cpp',
  ccm: 'cpp',
  cxxm: 'cpp',
  'c++m': 'cpp',
  txt: 'plaintext',
  csv: 'plaintext',
};

// Build supported extensions from FileTypeMap where app === 'code'
export const codeFileExtensions = Object.values(FileTypeMap)
  .filter((fileType) => fileType.app === 'code')
  .map((fileType) => fileType.extension)
  .sort();

export type CodeFileExtension = (typeof codeFileExtensions)[number];

// Combine with our explicitly mapped extensions
const allSupportedExtensions = [
  ...new Set([...Object.keys(extensionToLanguage), ...codeFileExtensions]),
];

// Supported file extensions (derived from FileTypeMap)
export const supportedExtensions = allSupportedExtensions;
export const supportedExtensionSet = new Set(supportedExtensions);
