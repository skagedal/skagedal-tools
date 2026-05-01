export interface LogEntry {
  id: number;
  raw: string;
  data: Record<string, unknown>;
  wrapped: boolean;
}

export interface ManagedField {
  name: string;
  from: string[];
  visible: boolean;
}

export interface MetaConfigField {
  name: string;
  from: string[];
}

export interface MetaResponse {
  sourceLabel: string;
  config: {
    fields: MetaConfigField[];
    defaultField?: string;
  };
}
