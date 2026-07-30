import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { execSync } from 'node:child_process';
import { parse } from 'smol-toml';

export interface CustomCommand {
  name: string; // The command name, e.g. "/review-and-fix"
  description: string;
  prompt: string;
}

export class CommandRegistry {
  private commands = new Map<string, CustomCommand>();

  constructor() {
    this.loadCommands();
  }

  private loadCommands() {
    const cwd = process.cwd();
    const home = os.homedir();

    const pathsToSearch = [
      path.join(home, '.sentinel', 'commands'),
      path.join(cwd, '.sentinel', 'commands'),
    ];

    for (const dir of pathsToSearch) {
      if (!fs.existsSync(dir)) continue;

      try {
        const files = fs.readdirSync(dir);
        for (const file of files) {
          if (!file.endsWith('.toml')) continue;

          const name = `/${file.replace(/\.toml$/, '')}`;
          const filePath = path.join(dir, file);
          
          try {
            const content = fs.readFileSync(filePath, 'utf-8');
            const parsed = parse(content) as any;
            
            if (parsed.prompt) {
              this.commands.set(name, {
                name,
                description: parsed.description || 'Custom command',
                prompt: parsed.prompt,
              });
            }
          } catch (err) {
            console.error(`Failed to parse command ${filePath}:`, err);
          }
        }
      } catch (err) {
        console.error(`Failed to read commands directory ${dir}:`, err);
      }
    }
  }

  public getCommand(name: string): CustomCommand | undefined {
    return this.commands.get(name);
  }

  public getHelpText(): string {
    if (this.commands.size === 0) return '';
    
    let help = '\nCustom commands:\n';
    for (const [name, cmd] of this.commands.entries()) {
      help += `  ${name.padEnd(16)} - ${cmd.description}\n`;
    }
    return help;
  }
}

export class CommandExpander {
  public static expand(template: string, args: string): string {
    let result = template;
    
    // Replace {{args}}
    result = result.replace(/\{\{args\}\}/g, args);
    
    // Replace !{cmd}
    const regex = /!\{([^}]+)\}/g;
    let match;
    
    // We have to iterate since we might replace multiple commands
    const matches: Array<{ full: string; cmd: string }> = [];
    while ((match = regex.exec(template)) !== null) {
      matches.push({ full: match[0], cmd: match[1].trim() });
    }
    
    for (const m of matches) {
      try {
        const output = execSync(m.cmd, { 
          cwd: process.cwd(), 
          encoding: 'utf-8',
          stdio: ['ignore', 'pipe', 'pipe']
        }).trim();
        result = result.replace(m.full, output);
      } catch (err: any) {
        // If command fails, insert the error so the LLM knows it failed
        const errMsg = err.stderr ? err.stderr.trim() : err.message;
        result = result.replace(m.full, `[Command Failed: ${m.cmd}]\n${errMsg}`);
      }
    }
    
    return result;
  }
}
