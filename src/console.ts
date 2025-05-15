export const warn = (message: string) => {
	console.warn(`\x1b[33m⚠️  ${message}\x1b[0m`);
};

export const info = (message: string) => {
	console.info(`\x1b[32m${message}\x1b[0m`);
};
