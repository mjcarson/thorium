module.exports = {
  'env': {
    'browser': true,
    'es2021': true,
  },
  'extends': [
    'plugin:react/recommended',
    'plugin:jsdoc/recommended',
    'plugin:react-hooks/recommended',
    'google',
  ],
  'parserOptions': {
    'ecmaFeatures': {
      'jsx': true,
    },
    'ecmaVersion': 12,
    'sourceType': 'module',
  },
  'plugins': [
    'react',
    'jsdoc',
  ],
  'rules': {
    'max-len': ['error', 100, 2, {ignoreUrls: true}],
    'react/prop-types': 'off',
    'valid-jsdoc': [2, {
      'prefer': {
        'return': 'returns',
      },
    }],
  },
  'settings': {
    'react': {
      'version': 'detect',
    },
  },
};
