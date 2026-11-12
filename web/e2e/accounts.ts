// The administrator the setup test creates and the flows sign in as.
export const ADMIN = {
  fullName: 'Erin Admin',
  username: 'erin',
  email: 'erin@example.com',
  password: 'correct horse battery',
} as const;

/** The setup code the server is started with; the wizard asks for it. */
export const SETUP_CODE = 'E2E0-TEST-CODE';
