import { handleOperator } from './handler.js';

export interface OperatorBindings {
  ADMIN: {
    inspect(): Promise<string>;
    submit(command: string): Promise<string>;
  };
  OPERATOR_TOKEN: string;
}

export default {
  fetch(request: Request, env: OperatorBindings): Promise<Response> {
    return handleOperator(request, env);
  },
};
