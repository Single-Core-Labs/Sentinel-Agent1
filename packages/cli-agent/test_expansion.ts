import { CommandExpander } from './src/commands.ts';

const template = `Echoing the args: {{args}}
And executing a command: !{echo "hello world"}`;

const result = CommandExpander.expand(template, 'my args');
console.log(result);
