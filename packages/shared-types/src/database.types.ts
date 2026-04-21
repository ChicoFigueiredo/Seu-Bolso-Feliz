export type Json = string | number | boolean | null | { [key: string]: Json | undefined } | Json[];

export type Database = {
  graphql_public: {
    Tables: {
      [_ in never]: never;
    };
    Views: {
      [_ in never]: never;
    };
    Functions: {
      graphql: {
        Args: {
          extensions?: Json;
          operationName?: string;
          query?: string;
          variables?: Json;
        };
        Returns: Json;
      };
    };
    Enums: {
      [_ in never]: never;
    };
    CompositeTypes: {
      [_ in never]: never;
    };
  };
  public: {
    Tables: {
      ai_chat_messages: {
        Row: {
          content: string | null;
          created_at: string | null;
          id: string;
          latency_ms: number | null;
          model: string | null;
          role: string;
          session_id: string;
          tokens_used: number | null;
          tool_call_id: string | null;
          tool_calls: Json | null;
          tool_name: string | null;
          user_id: string;
        };
        Insert: {
          content?: string | null;
          created_at?: string | null;
          id?: string;
          latency_ms?: number | null;
          model?: string | null;
          role: string;
          session_id: string;
          tokens_used?: number | null;
          tool_call_id?: string | null;
          tool_calls?: Json | null;
          tool_name?: string | null;
          user_id: string;
        };
        Update: {
          content?: string | null;
          created_at?: string | null;
          id?: string;
          latency_ms?: number | null;
          model?: string | null;
          role?: string;
          session_id?: string;
          tokens_used?: number | null;
          tool_call_id?: string | null;
          tool_calls?: Json | null;
          tool_name?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "ai_chat_messages_session_id_fkey";
            columns: ["session_id"];
            isOneToOne: false;
            referencedRelation: "ai_chat_sessions";
            referencedColumns: ["id"];
          },
        ];
      };
      ai_chat_sessions: {
        Row: {
          context_id: string | null;
          context_type: string | null;
          created_at: string | null;
          id: string;
          message_count: number | null;
          model: string;
          title: string | null;
          total_tokens_used: number | null;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          context_id?: string | null;
          context_type?: string | null;
          created_at?: string | null;
          id?: string;
          message_count?: number | null;
          model?: string;
          title?: string | null;
          total_tokens_used?: number | null;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          context_id?: string | null;
          context_type?: string | null;
          created_at?: string | null;
          id?: string;
          message_count?: number | null;
          model?: string;
          title?: string | null;
          total_tokens_used?: number | null;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [];
      };
      audit_logs: {
        Row: {
          action: string;
          created_at: string;
          entity_id: string | null;
          entity_type: string | null;
          id: string;
          ip_address: unknown;
          new_values: Json | null;
          old_values: Json | null;
          user_id: string;
        };
        Insert: {
          action: string;
          created_at?: string;
          entity_id?: string | null;
          entity_type?: string | null;
          id?: string;
          ip_address?: unknown;
          new_values?: Json | null;
          old_values?: Json | null;
          user_id: string;
        };
        Update: {
          action?: string;
          created_at?: string;
          entity_id?: string | null;
          entity_type?: string | null;
          id?: string;
          ip_address?: unknown;
          new_values?: Json | null;
          old_values?: Json | null;
          user_id?: string;
        };
        Relationships: [];
      };
      cards: {
        Row: {
          card_brand: string | null;
          closing_day: number | null;
          created_at: string;
          credit_limit: number | null;
          due_day: number | null;
          financial_product_id: string;
          holder_name: string | null;
          id: string;
          is_active: boolean;
          is_primary: boolean;
          last_four_digits: string | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          card_brand?: string | null;
          closing_day?: number | null;
          created_at?: string;
          credit_limit?: number | null;
          due_day?: number | null;
          financial_product_id: string;
          holder_name?: string | null;
          id?: string;
          is_active?: boolean;
          is_primary?: boolean;
          last_four_digits?: string | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          card_brand?: string | null;
          closing_day?: number | null;
          created_at?: string;
          credit_limit?: number | null;
          due_day?: number | null;
          financial_product_id?: string;
          holder_name?: string | null;
          id?: string;
          is_active?: boolean;
          is_primary?: boolean;
          last_four_digits?: string | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "cards_financial_product_id_fkey";
            columns: ["financial_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
        ];
      };
      categories: {
        Row: {
          color: string | null;
          created_at: string;
          display_order: number | null;
          icon: string | null;
          id: string;
          is_active: boolean;
          name: string;
          parent_id: string | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          color?: string | null;
          created_at?: string;
          display_order?: number | null;
          icon?: string | null;
          id?: string;
          is_active?: boolean;
          name: string;
          parent_id?: string | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          color?: string | null;
          created_at?: string;
          display_order?: number | null;
          icon?: string | null;
          id?: string;
          is_active?: boolean;
          name?: string;
          parent_id?: string | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "categories_parent_id_fkey";
            columns: ["parent_id"];
            isOneToOne: false;
            referencedRelation: "categories";
            referencedColumns: ["id"];
          },
        ];
      };
      consumption_metrics: {
        Row: {
          created_at: string;
          document_id: string | null;
          id: string;
          metadata: Json | null;
          metric_name: string | null;
          metric_unit: string | null;
          quantity: number | null;
          reference_period_end: string;
          reference_period_start: string;
          subtotal: number | null;
          supplier_contract_id: string | null;
          supplier_id: string;
          transaction_id: string | null;
          unit_price: number | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          document_id?: string | null;
          id?: string;
          metadata?: Json | null;
          metric_name?: string | null;
          metric_unit?: string | null;
          quantity?: number | null;
          reference_period_end: string;
          reference_period_start: string;
          subtotal?: number | null;
          supplier_contract_id?: string | null;
          supplier_id: string;
          transaction_id?: string | null;
          unit_price?: number | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          document_id?: string | null;
          id?: string;
          metadata?: Json | null;
          metric_name?: string | null;
          metric_unit?: string | null;
          quantity?: number | null;
          reference_period_end?: string;
          reference_period_start?: string;
          subtotal?: number | null;
          supplier_contract_id?: string | null;
          supplier_id?: string;
          transaction_id?: string | null;
          unit_price?: number | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "consumption_metrics_document_id_fkey";
            columns: ["document_id"];
            isOneToOne: false;
            referencedRelation: "documents";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "consumption_metrics_supplier_contract_id_fkey";
            columns: ["supplier_contract_id"];
            isOneToOne: false;
            referencedRelation: "supplier_contracts";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "consumption_metrics_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "consumption_metrics_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "consumption_metrics_transaction_id_fkey";
            columns: ["transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
        ];
      };
      document_fingerprints: {
        Row: {
          canonical_fingerprint: string | null;
          content_hash: string;
          created_at: string | null;
          hash_algorithm: string | null;
          id: string;
          source_document_id: string;
          user_id: string;
        };
        Insert: {
          canonical_fingerprint?: string | null;
          content_hash: string;
          created_at?: string | null;
          hash_algorithm?: string | null;
          id?: string;
          source_document_id: string;
          user_id: string;
        };
        Update: {
          canonical_fingerprint?: string | null;
          content_hash?: string;
          created_at?: string | null;
          hash_algorithm?: string | null;
          id?: string;
          source_document_id?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "document_fingerprints_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      document_patterns: {
        Row: {
          confidence_threshold: number;
          created_at: string;
          document_type: string;
          extraction_rules: Json;
          feedback_count: number;
          field_mappings: Json;
          id: string;
          institution_id: string | null;
          is_active: boolean;
          name: string;
          sample_fingerprints: string[];
          success_count: number;
          supplier_id: string | null;
          updated_at: string;
          user_id: string;
          version: number;
        };
        Insert: {
          confidence_threshold?: number;
          created_at?: string;
          document_type: string;
          extraction_rules?: Json;
          feedback_count?: number;
          field_mappings?: Json;
          id?: string;
          institution_id?: string | null;
          is_active?: boolean;
          name: string;
          sample_fingerprints?: string[];
          success_count?: number;
          supplier_id?: string | null;
          updated_at?: string;
          user_id: string;
          version?: number;
        };
        Update: {
          confidence_threshold?: number;
          created_at?: string;
          document_type?: string;
          extraction_rules?: Json;
          feedback_count?: number;
          field_mappings?: Json;
          id?: string;
          institution_id?: string | null;
          is_active?: boolean;
          name?: string;
          sample_fingerprints?: string[];
          success_count?: number;
          supplier_id?: string | null;
          updated_at?: string;
          user_id?: string;
          version?: number;
        };
        Relationships: [
          {
            foreignKeyName: "document_patterns_institution_id_fkey";
            columns: ["institution_id"];
            isOneToOne: false;
            referencedRelation: "institutions";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "document_patterns_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "document_patterns_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      document_splits: {
        Row: {
          amount: number;
          category_id: string | null;
          created_at: string;
          description: string | null;
          id: string;
          source_document_id: string;
          tags: string[];
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amount: number;
          category_id?: string | null;
          created_at?: string;
          description?: string | null;
          id?: string;
          source_document_id: string;
          tags?: string[];
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amount?: number;
          category_id?: string | null;
          created_at?: string;
          description?: string | null;
          id?: string;
          source_document_id?: string;
          tags?: string[];
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "document_splits_category_id_fkey";
            columns: ["category_id"];
            isOneToOne: false;
            referencedRelation: "categories";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "document_splits_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      document_transactions: {
        Row: {
          confidence: number | null;
          created_at: string;
          created_by: string;
          id: string;
          link_type: string;
          notes: string | null;
          source_document_id: string;
          transaction_id: string;
          user_id: string;
        };
        Insert: {
          confidence?: number | null;
          created_at?: string;
          created_by?: string;
          id?: string;
          link_type: string;
          notes?: string | null;
          source_document_id: string;
          transaction_id: string;
          user_id: string;
        };
        Update: {
          confidence?: number | null;
          created_at?: string;
          created_by?: string;
          id?: string;
          link_type?: string;
          notes?: string | null;
          source_document_id?: string;
          transaction_id?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "document_transactions_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "document_transactions_transaction_id_fkey";
            columns: ["transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
        ];
      };
      documents: {
        Row: {
          created_at: string;
          description: string | null;
          document_type: string | null;
          entity_id: string | null;
          entity_type: string | null;
          file_path: string;
          file_size: number | null;
          file_type: string | null;
          id: string;
          is_password_protected: boolean;
          name: string;
          supplier_id: string | null;
          updated_at: string;
          user_id: string;
          version: number;
        };
        Insert: {
          created_at?: string;
          description?: string | null;
          document_type?: string | null;
          entity_id?: string | null;
          entity_type?: string | null;
          file_path: string;
          file_size?: number | null;
          file_type?: string | null;
          id?: string;
          is_password_protected?: boolean;
          name: string;
          supplier_id?: string | null;
          updated_at?: string;
          user_id: string;
          version?: number;
        };
        Update: {
          created_at?: string;
          description?: string | null;
          document_type?: string | null;
          entity_id?: string | null;
          entity_type?: string | null;
          file_path?: string;
          file_size?: number | null;
          file_type?: string | null;
          id?: string;
          is_password_protected?: boolean;
          name?: string;
          supplier_id?: string | null;
          updated_at?: string;
          user_id?: string;
          version?: number;
        };
        Relationships: [
          {
            foreignKeyName: "documents_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "documents_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      draft_batches: {
        Row: {
          approved_count: number | null;
          created_at: string | null;
          id: string;
          name: string | null;
          rejected_count: number | null;
          run_id: string | null;
          source_document_id: string | null;
          status: string;
          total_drafts: number | null;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          approved_count?: number | null;
          created_at?: string | null;
          id?: string;
          name?: string | null;
          rejected_count?: number | null;
          run_id?: string | null;
          source_document_id?: string | null;
          status?: string;
          total_drafts?: number | null;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          approved_count?: number | null;
          created_at?: string | null;
          id?: string;
          name?: string | null;
          rejected_count?: number | null;
          run_id?: string | null;
          source_document_id?: string | null;
          status?: string;
          total_drafts?: number | null;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "draft_batches_run_id_fkey";
            columns: ["run_id"];
            isOneToOne: false;
            referencedRelation: "ingestion_runs";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "draft_batches_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      draft_records: {
        Row: {
          approved_at: string | null;
          approved_by: string | null;
          batch_id: string | null;
          confidence_score: number | null;
          corrections: Json | null;
          created_at: string | null;
          draft_data: Json;
          draft_type: string;
          extraction_result_id: string | null;
          id: string;
          posted_record_id: string | null;
          posted_record_type: string | null;
          reconciled_at: string | null;
          reconciled_template_id: string | null;
          reconciled_transaction_id: string | null;
          reconciliation_candidates: Json | null;
          reconciliation_status: string;
          source_document_id: string | null;
          status: string;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          approved_at?: string | null;
          approved_by?: string | null;
          batch_id?: string | null;
          confidence_score?: number | null;
          corrections?: Json | null;
          created_at?: string | null;
          draft_data: Json;
          draft_type: string;
          extraction_result_id?: string | null;
          id?: string;
          posted_record_id?: string | null;
          posted_record_type?: string | null;
          reconciled_at?: string | null;
          reconciled_template_id?: string | null;
          reconciled_transaction_id?: string | null;
          reconciliation_candidates?: Json | null;
          reconciliation_status?: string;
          source_document_id?: string | null;
          status?: string;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          approved_at?: string | null;
          approved_by?: string | null;
          batch_id?: string | null;
          confidence_score?: number | null;
          corrections?: Json | null;
          created_at?: string | null;
          draft_data?: Json;
          draft_type?: string;
          extraction_result_id?: string | null;
          id?: string;
          posted_record_id?: string | null;
          posted_record_type?: string | null;
          reconciled_at?: string | null;
          reconciled_template_id?: string | null;
          reconciled_transaction_id?: string | null;
          reconciliation_candidates?: Json | null;
          reconciliation_status?: string;
          source_document_id?: string | null;
          status?: string;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "draft_records_batch_id_fkey";
            columns: ["batch_id"];
            isOneToOne: false;
            referencedRelation: "draft_batches";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "draft_records_extraction_result_id_fkey";
            columns: ["extraction_result_id"];
            isOneToOne: false;
            referencedRelation: "extraction_results";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "draft_records_reconciled_template_id_fkey";
            columns: ["reconciled_template_id"];
            isOneToOne: false;
            referencedRelation: "recurring_templates";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "draft_records_reconciled_transaction_id_fkey";
            columns: ["reconciled_transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "draft_records_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      extraction_results: {
        Row: {
          breakdown: Json | null;
          category_suggestion: string | null;
          competence_date: string | null;
          consumption_data: Json | null;
          contract_identifier: string | null;
          created_at: string | null;
          currency: string | null;
          document_number: string | null;
          due_date: string | null;
          financial_period_suggestion: Json | null;
          id: string;
          metadata: Json | null;
          parsed_version_id: string;
          priority_suggestion: string | null;
          supplier_confidence: number | null;
          supplier_id: string | null;
          supplier_name_raw: string | null;
          tags_suggestion: string[] | null;
          total_amount: number | null;
          user_id: string;
        };
        Insert: {
          breakdown?: Json | null;
          category_suggestion?: string | null;
          competence_date?: string | null;
          consumption_data?: Json | null;
          contract_identifier?: string | null;
          created_at?: string | null;
          currency?: string | null;
          document_number?: string | null;
          due_date?: string | null;
          financial_period_suggestion?: Json | null;
          id?: string;
          metadata?: Json | null;
          parsed_version_id: string;
          priority_suggestion?: string | null;
          supplier_confidence?: number | null;
          supplier_id?: string | null;
          supplier_name_raw?: string | null;
          tags_suggestion?: string[] | null;
          total_amount?: number | null;
          user_id: string;
        };
        Update: {
          breakdown?: Json | null;
          category_suggestion?: string | null;
          competence_date?: string | null;
          consumption_data?: Json | null;
          contract_identifier?: string | null;
          created_at?: string | null;
          currency?: string | null;
          document_number?: string | null;
          due_date?: string | null;
          financial_period_suggestion?: Json | null;
          id?: string;
          metadata?: Json | null;
          parsed_version_id?: string;
          priority_suggestion?: string | null;
          supplier_confidence?: number | null;
          supplier_id?: string | null;
          supplier_name_raw?: string | null;
          tags_suggestion?: string[] | null;
          total_amount?: number | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "extraction_results_parsed_version_id_fkey";
            columns: ["parsed_version_id"];
            isOneToOne: false;
            referencedRelation: "parsed_document_versions";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "extraction_results_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "extraction_results_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      financial_periods: {
        Row: {
          created_at: string;
          end_date: string;
          id: string;
          is_current: boolean;
          label: string | null;
          start_date: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          end_date: string;
          id?: string;
          is_current?: boolean;
          label?: string | null;
          start_date: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          end_date?: string;
          id?: string;
          is_current?: boolean;
          label?: string | null;
          start_date?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
      financial_products: {
        Row: {
          created_at: string;
          credit_limit: number | null;
          current_balance: number | null;
          display_order: number | null;
          id: string;
          institution_id: string;
          is_active: boolean;
          metadata: Json | null;
          name: string;
          type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          credit_limit?: number | null;
          current_balance?: number | null;
          display_order?: number | null;
          id?: string;
          institution_id: string;
          is_active?: boolean;
          metadata?: Json | null;
          name: string;
          type: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          credit_limit?: number | null;
          current_balance?: number | null;
          display_order?: number | null;
          id?: string;
          institution_id?: string;
          is_active?: boolean;
          metadata?: Json | null;
          name?: string;
          type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "financial_products_institution_id_fkey";
            columns: ["institution_id"];
            isOneToOne: false;
            referencedRelation: "institutions";
            referencedColumns: ["id"];
          },
        ];
      };
      import_jobs: {
        Row: {
          completed_at: string | null;
          created_at: string;
          error_details: Json | null;
          error_rows: number;
          file_path: string | null;
          id: string;
          imported_rows: number;
          skipped_rows: number;
          source_type: string;
          started_at: string | null;
          status: string;
          total_rows: number | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          completed_at?: string | null;
          created_at?: string;
          error_details?: Json | null;
          error_rows?: number;
          file_path?: string | null;
          id?: string;
          imported_rows?: number;
          skipped_rows?: number;
          source_type: string;
          started_at?: string | null;
          status?: string;
          total_rows?: number | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          completed_at?: string | null;
          created_at?: string;
          error_details?: Json | null;
          error_rows?: number;
          file_path?: string | null;
          id?: string;
          imported_rows?: number;
          skipped_rows?: number;
          source_type?: string;
          started_at?: string | null;
          status?: string;
          total_rows?: number | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
      ingestion_jobs: {
        Row: {
          created_at: string | null;
          error_details: Json | null;
          error_message: string | null;
          id: string;
          max_retries: number | null;
          metadata: Json | null;
          retry_count: number | null;
          run_id: string;
          source_document_id: string | null;
          status: string;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          created_at?: string | null;
          error_details?: Json | null;
          error_message?: string | null;
          id?: string;
          max_retries?: number | null;
          metadata?: Json | null;
          retry_count?: number | null;
          run_id: string;
          source_document_id?: string | null;
          status?: string;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          created_at?: string | null;
          error_details?: Json | null;
          error_message?: string | null;
          id?: string;
          max_retries?: number | null;
          metadata?: Json | null;
          retry_count?: number | null;
          run_id?: string;
          source_document_id?: string | null;
          status?: string;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "ingestion_jobs_run_id_fkey";
            columns: ["run_id"];
            isOneToOne: false;
            referencedRelation: "ingestion_runs";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "ingestion_jobs_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      ingestion_logs: {
        Row: {
          created_at: string | null;
          details: Json | null;
          id: string;
          job_id: string | null;
          level: string;
          message: string;
          run_id: string | null;
          user_id: string;
        };
        Insert: {
          created_at?: string | null;
          details?: Json | null;
          id?: string;
          job_id?: string | null;
          level?: string;
          message: string;
          run_id?: string | null;
          user_id: string;
        };
        Update: {
          created_at?: string | null;
          details?: Json | null;
          id?: string;
          job_id?: string | null;
          level?: string;
          message?: string;
          run_id?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "ingestion_logs_job_id_fkey";
            columns: ["job_id"];
            isOneToOne: false;
            referencedRelation: "ingestion_jobs";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "ingestion_logs_run_id_fkey";
            columns: ["run_id"];
            isOneToOne: false;
            referencedRelation: "ingestion_runs";
            referencedColumns: ["id"];
          },
        ];
      };
      ingestion_runs: {
        Row: {
          completed_at: string | null;
          created_at: string | null;
          id: string;
          metadata: Json | null;
          source_type: string;
          started_at: string;
          stats: Json | null;
          status: string;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          completed_at?: string | null;
          created_at?: string | null;
          id?: string;
          metadata?: Json | null;
          source_type: string;
          started_at?: string;
          stats?: Json | null;
          status?: string;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          completed_at?: string | null;
          created_at?: string | null;
          id?: string;
          metadata?: Json | null;
          source_type?: string;
          started_at?: string;
          stats?: Json | null;
          status?: string;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [];
      };
      institutions: {
        Row: {
          color: string | null;
          created_at: string;
          display_order: number | null;
          icon_url: string | null;
          id: string;
          is_active: boolean;
          name: string;
          type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          color?: string | null;
          created_at?: string;
          display_order?: number | null;
          icon_url?: string | null;
          id?: string;
          is_active?: boolean;
          name: string;
          type?: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          color?: string | null;
          created_at?: string;
          display_order?: number | null;
          icon_url?: string | null;
          id?: string;
          is_active?: boolean;
          name?: string;
          type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
      liabilities: {
        Row: {
          amortization_system: string | null;
          created_at: string;
          end_date: string | null;
          financial_product_id: string;
          id: string;
          interest_rate: number | null;
          metadata: Json | null;
          name: string;
          original_amount: number;
          outstanding_balance: number;
          paid_installments: number;
          rate_type: string | null;
          start_date: string | null;
          status: string;
          supplier_id: string | null;
          total_installments: number | null;
          type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amortization_system?: string | null;
          created_at?: string;
          end_date?: string | null;
          financial_product_id: string;
          id?: string;
          interest_rate?: number | null;
          metadata?: Json | null;
          name: string;
          original_amount: number;
          outstanding_balance: number;
          paid_installments?: number;
          rate_type?: string | null;
          start_date?: string | null;
          status?: string;
          supplier_id?: string | null;
          total_installments?: number | null;
          type: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amortization_system?: string | null;
          created_at?: string;
          end_date?: string | null;
          financial_product_id?: string;
          id?: string;
          interest_rate?: number | null;
          metadata?: Json | null;
          name?: string;
          original_amount?: number;
          outstanding_balance?: number;
          paid_installments?: number;
          rate_type?: string | null;
          start_date?: string | null;
          status?: string;
          supplier_id?: string | null;
          total_installments?: number | null;
          type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "liabilities_financial_product_id_fkey";
            columns: ["financial_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "liabilities_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "liabilities_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      liability_installments: {
        Row: {
          created_at: string;
          due_date: string;
          fee_amount: number | null;
          id: string;
          installment_number: number;
          insurance_amount: number | null;
          interest_amount: number | null;
          liability_id: string;
          paid_amount: number;
          paid_date: string | null;
          principal_amount: number | null;
          status: string;
          total_amount: number;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          due_date: string;
          fee_amount?: number | null;
          id?: string;
          installment_number: number;
          insurance_amount?: number | null;
          interest_amount?: number | null;
          liability_id: string;
          paid_amount?: number;
          paid_date?: string | null;
          principal_amount?: number | null;
          status?: string;
          total_amount: number;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          due_date?: string;
          fee_amount?: number | null;
          id?: string;
          installment_number?: number;
          insurance_amount?: number | null;
          interest_amount?: number | null;
          liability_id?: string;
          paid_amount?: number;
          paid_date?: string | null;
          principal_amount?: number | null;
          status?: string;
          total_amount?: number;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "liability_installments_liability_id_fkey";
            columns: ["liability_id"];
            isOneToOne: false;
            referencedRelation: "liabilities";
            referencedColumns: ["id"];
          },
        ];
      };
      liability_tags: {
        Row: {
          liability_id: string;
          tag_id: string;
        };
        Insert: {
          liability_id: string;
          tag_id: string;
        };
        Update: {
          liability_id?: string;
          tag_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "liability_tags_liability_id_fkey";
            columns: ["liability_id"];
            isOneToOne: false;
            referencedRelation: "liabilities";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "liability_tags_tag_id_fkey";
            columns: ["tag_id"];
            isOneToOne: false;
            referencedRelation: "tags";
            referencedColumns: ["id"];
          },
        ];
      };
      parsed_document_versions: {
        Row: {
          confidence_score: number | null;
          created_at: string | null;
          id: string;
          metadata: Json | null;
          parser_type: string;
          parser_version: string | null;
          raw_text: string | null;
          source_document_id: string;
          structured_data: Json | null;
          user_id: string;
          version_number: number;
        };
        Insert: {
          confidence_score?: number | null;
          created_at?: string | null;
          id?: string;
          metadata?: Json | null;
          parser_type: string;
          parser_version?: string | null;
          raw_text?: string | null;
          source_document_id: string;
          structured_data?: Json | null;
          user_id: string;
          version_number?: number;
        };
        Update: {
          confidence_score?: number | null;
          created_at?: string | null;
          id?: string;
          metadata?: Json | null;
          parser_type?: string;
          parser_version?: string | null;
          raw_text?: string | null;
          source_document_id?: string;
          structured_data?: Json | null;
          user_id?: string;
          version_number?: number;
        };
        Relationships: [
          {
            foreignKeyName: "parsed_document_versions_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      pattern_feedback: {
        Row: {
          corrections: Json;
          created_at: string;
          feedback_type: string;
          id: string;
          notes: string | null;
          pattern_id: string;
          source_document_id: string | null;
          user_id: string;
        };
        Insert: {
          corrections?: Json;
          created_at?: string;
          feedback_type: string;
          id?: string;
          notes?: string | null;
          pattern_id: string;
          source_document_id?: string | null;
          user_id: string;
        };
        Update: {
          corrections?: Json;
          created_at?: string;
          feedback_type?: string;
          id?: string;
          notes?: string | null;
          pattern_id?: string;
          source_document_id?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "pattern_feedback_pattern_id_fkey";
            columns: ["pattern_id"];
            isOneToOne: false;
            referencedRelation: "document_patterns";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "pattern_feedback_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
        ];
      };
      recurring_instances: {
        Row: {
          actual_amount: number | null;
          created_at: string;
          expected_amount: number | null;
          expected_date: string;
          id: string;
          notes: string | null;
          paid_date: string | null;
          recurring_template_id: string;
          status: string;
          transaction_id: string | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          actual_amount?: number | null;
          created_at?: string;
          expected_amount?: number | null;
          expected_date: string;
          id?: string;
          notes?: string | null;
          paid_date?: string | null;
          recurring_template_id: string;
          status?: string;
          transaction_id?: string | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          actual_amount?: number | null;
          created_at?: string;
          expected_amount?: number | null;
          expected_date?: string;
          id?: string;
          notes?: string | null;
          paid_date?: string | null;
          recurring_template_id?: string;
          status?: string;
          transaction_id?: string | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "recurring_instances_recurring_template_id_fkey";
            columns: ["recurring_template_id"];
            isOneToOne: false;
            referencedRelation: "recurring_templates";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "recurring_instances_transaction_id_fkey";
            columns: ["transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
        ];
      };
      recurring_template_tags: {
        Row: {
          recurring_template_id: string;
          tag_id: string;
        };
        Insert: {
          recurring_template_id: string;
          tag_id: string;
        };
        Update: {
          recurring_template_id?: string;
          tag_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "recurring_template_tags_recurring_template_id_fkey";
            columns: ["recurring_template_id"];
            isOneToOne: false;
            referencedRelation: "recurring_templates";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "recurring_template_tags_tag_id_fkey";
            columns: ["tag_id"];
            isOneToOne: false;
            referencedRelation: "tags";
            referencedColumns: ["id"];
          },
        ];
      };
      recurring_templates: {
        Row: {
          amount: number | null;
          category_id: string | null;
          created_at: string;
          custom_interval_days: number | null;
          day_of_month: number | null;
          ends_at: string | null;
          financial_product_id: string | null;
          frequency: string;
          id: string;
          is_active: boolean;
          is_variable_amount: boolean;
          name: string;
          notes: string | null;
          priority: string | null;
          starts_at: string | null;
          supplier_id: string | null;
          type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amount?: number | null;
          category_id?: string | null;
          created_at?: string;
          custom_interval_days?: number | null;
          day_of_month?: number | null;
          ends_at?: string | null;
          financial_product_id?: string | null;
          frequency: string;
          id?: string;
          is_active?: boolean;
          is_variable_amount?: boolean;
          name: string;
          notes?: string | null;
          priority?: string | null;
          starts_at?: string | null;
          supplier_id?: string | null;
          type: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amount?: number | null;
          category_id?: string | null;
          created_at?: string;
          custom_interval_days?: number | null;
          day_of_month?: number | null;
          ends_at?: string | null;
          financial_product_id?: string | null;
          frequency?: string;
          id?: string;
          is_active?: boolean;
          is_variable_amount?: boolean;
          name?: string;
          notes?: string | null;
          priority?: string | null;
          starts_at?: string | null;
          supplier_id?: string | null;
          type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "recurring_templates_category_id_fkey";
            columns: ["category_id"];
            isOneToOne: false;
            referencedRelation: "categories";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "recurring_templates_financial_product_id_fkey";
            columns: ["financial_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "recurring_templates_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "recurring_templates_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      source_documents: {
        Row: {
          content_hash: string | null;
          created_at: string | null;
          document_type: string | null;
          file_size_bytes: number | null;
          filename: string;
          gmail_attachment_id: string | null;
          gmail_date: string | null;
          gmail_from: string | null;
          gmail_label: string | null;
          gmail_message_id: string | null;
          gmail_subject: string | null;
          gmail_thread_id: string | null;
          id: string;
          local_filepath: string | null;
          local_mtime: string | null;
          metadata: Json | null;
          mime_type: string | null;
          origin_key: string;
          origin_type: string;
          status: string;
          storage_path: string | null;
          supplier_id: string | null;
          supplier_name_raw: string | null;
          updated_at: string | null;
          user_id: string;
        };
        Insert: {
          content_hash?: string | null;
          created_at?: string | null;
          document_type?: string | null;
          file_size_bytes?: number | null;
          filename: string;
          gmail_attachment_id?: string | null;
          gmail_date?: string | null;
          gmail_from?: string | null;
          gmail_label?: string | null;
          gmail_message_id?: string | null;
          gmail_subject?: string | null;
          gmail_thread_id?: string | null;
          id?: string;
          local_filepath?: string | null;
          local_mtime?: string | null;
          metadata?: Json | null;
          mime_type?: string | null;
          origin_key: string;
          origin_type: string;
          status?: string;
          storage_path?: string | null;
          supplier_id?: string | null;
          supplier_name_raw?: string | null;
          updated_at?: string | null;
          user_id: string;
        };
        Update: {
          content_hash?: string | null;
          created_at?: string | null;
          document_type?: string | null;
          file_size_bytes?: number | null;
          filename?: string;
          gmail_attachment_id?: string | null;
          gmail_date?: string | null;
          gmail_from?: string | null;
          gmail_label?: string | null;
          gmail_message_id?: string | null;
          gmail_subject?: string | null;
          gmail_thread_id?: string | null;
          id?: string;
          local_filepath?: string | null;
          local_mtime?: string | null;
          metadata?: Json | null;
          mime_type?: string | null;
          origin_key?: string;
          origin_type?: string;
          status?: string;
          storage_path?: string | null;
          supplier_id?: string | null;
          supplier_name_raw?: string | null;
          updated_at?: string | null;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "source_documents_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "source_documents_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      statement_cycles: {
        Row: {
          card_id: string;
          created_at: string;
          cycle_end_date: string;
          cycle_start_date: string;
          due_date: string;
          id: string;
          paid_amount: number | null;
          reference_month: string;
          status: string;
          total_amount: number | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          card_id: string;
          created_at?: string;
          cycle_end_date: string;
          cycle_start_date: string;
          due_date: string;
          id?: string;
          paid_amount?: number | null;
          reference_month: string;
          status?: string;
          total_amount?: number | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          card_id?: string;
          created_at?: string;
          cycle_end_date?: string;
          cycle_start_date?: string;
          due_date?: string;
          id?: string;
          paid_amount?: number | null;
          reference_month?: string;
          status?: string;
          total_amount?: number | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "statement_cycles_card_id_fkey";
            columns: ["card_id"];
            isOneToOne: false;
            referencedRelation: "cards";
            referencedColumns: ["id"];
          },
        ];
      };
      statement_items: {
        Row: {
          amount: number;
          created_at: string;
          description: string | null;
          id: string;
          installment_number: number | null;
          statement_cycle_id: string;
          supplier_id: string | null;
          total_installments: number | null;
          transaction_date: string | null;
          transaction_id: string | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amount: number;
          created_at?: string;
          description?: string | null;
          id?: string;
          installment_number?: number | null;
          statement_cycle_id: string;
          supplier_id?: string | null;
          total_installments?: number | null;
          transaction_date?: string | null;
          transaction_id?: string | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amount?: number;
          created_at?: string;
          description?: string | null;
          id?: string;
          installment_number?: number | null;
          statement_cycle_id?: string;
          supplier_id?: string | null;
          total_installments?: number | null;
          transaction_date?: string | null;
          transaction_id?: string | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "statement_items_statement_cycle_id_fkey";
            columns: ["statement_cycle_id"];
            isOneToOne: false;
            referencedRelation: "statement_cycles";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "statement_items_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "statement_items_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "statement_items_transaction_id_fkey";
            columns: ["transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
        ];
      };
      supplier_aliases: {
        Row: {
          alias_name: string;
          alias_type: string;
          created_at: string;
          id: string;
          is_active: boolean;
          notes: string | null;
          supplier_id: string;
          updated_at: string;
          user_id: string;
          valid_from: string | null;
          valid_until: string | null;
        };
        Insert: {
          alias_name: string;
          alias_type?: string;
          created_at?: string;
          id?: string;
          is_active?: boolean;
          notes?: string | null;
          supplier_id: string;
          updated_at?: string;
          user_id: string;
          valid_from?: string | null;
          valid_until?: string | null;
        };
        Update: {
          alias_name?: string;
          alias_type?: string;
          created_at?: string;
          id?: string;
          is_active?: boolean;
          notes?: string | null;
          supplier_id?: string;
          updated_at?: string;
          user_id?: string;
          valid_from?: string | null;
          valid_until?: string | null;
        };
        Relationships: [
          {
            foreignKeyName: "supplier_aliases_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "supplier_aliases_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      supplier_contracts: {
        Row: {
          contract_type: string;
          created_at: string;
          end_date: string | null;
          id: string;
          identifier: string | null;
          is_active: boolean;
          label: string | null;
          metadata: Json | null;
          notes: string | null;
          start_date: string | null;
          supplier_id: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          contract_type: string;
          created_at?: string;
          end_date?: string | null;
          id?: string;
          identifier?: string | null;
          is_active?: boolean;
          label?: string | null;
          metadata?: Json | null;
          notes?: string | null;
          start_date?: string | null;
          supplier_id: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          contract_type?: string;
          created_at?: string;
          end_date?: string | null;
          id?: string;
          identifier?: string | null;
          is_active?: boolean;
          label?: string | null;
          metadata?: Json | null;
          notes?: string | null;
          start_date?: string | null;
          supplier_id?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "supplier_contracts_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "supplier_contracts_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      supplier_tags: {
        Row: {
          created_at: string;
          id: string;
          supplier_id: string;
          tag_id: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          id?: string;
          supplier_id: string;
          tag_id: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          id?: string;
          supplier_id?: string;
          tag_id?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "supplier_tags_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "supplier_tags_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "supplier_tags_tag_id_fkey";
            columns: ["tag_id"];
            isOneToOne: false;
            referencedRelation: "tags";
            referencedColumns: ["id"];
          },
        ];
      };
      suppliers: {
        Row: {
          contact_info: Json | null;
          created_at: string;
          display_order: number | null;
          document_number: string | null;
          id: string;
          institution_id: string | null;
          is_active: boolean;
          legal_name: string | null;
          name: string;
          notes: string | null;
          trade_name: string | null;
          type: string;
          updated_at: string;
          user_id: string;
          website: string | null;
        };
        Insert: {
          contact_info?: Json | null;
          created_at?: string;
          display_order?: number | null;
          document_number?: string | null;
          id?: string;
          institution_id?: string | null;
          is_active?: boolean;
          legal_name?: string | null;
          name: string;
          notes?: string | null;
          trade_name?: string | null;
          type?: string;
          updated_at?: string;
          user_id: string;
          website?: string | null;
        };
        Update: {
          contact_info?: Json | null;
          created_at?: string;
          display_order?: number | null;
          document_number?: string | null;
          id?: string;
          institution_id?: string | null;
          is_active?: boolean;
          legal_name?: string | null;
          name?: string;
          notes?: string | null;
          trade_name?: string | null;
          type?: string;
          updated_at?: string;
          user_id?: string;
          website?: string | null;
        };
        Relationships: [
          {
            foreignKeyName: "suppliers_institution_id_fkey";
            columns: ["institution_id"];
            isOneToOne: false;
            referencedRelation: "institutions";
            referencedColumns: ["id"];
          },
        ];
      };
      tags: {
        Row: {
          color: string | null;
          created_at: string;
          id: string;
          influences_priority: boolean;
          name: string;
          suggested_priority: string | null;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          color?: string | null;
          created_at?: string;
          id?: string;
          influences_priority?: boolean;
          name: string;
          suggested_priority?: string | null;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          color?: string | null;
          created_at?: string;
          id?: string;
          influences_priority?: boolean;
          name?: string;
          suggested_priority?: string | null;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
      transaction_tags: {
        Row: {
          tag_id: string;
          transaction_id: string;
        };
        Insert: {
          tag_id: string;
          transaction_id: string;
        };
        Update: {
          tag_id?: string;
          transaction_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "transaction_tags_tag_id_fkey";
            columns: ["tag_id"];
            isOneToOne: false;
            referencedRelation: "tags";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transaction_tags_transaction_id_fkey";
            columns: ["transaction_id"];
            isOneToOne: false;
            referencedRelation: "transactions";
            referencedColumns: ["id"];
          },
        ];
      };
      transactions: {
        Row: {
          amount: number;
          category_id: string | null;
          competence_date: string | null;
          created_at: string;
          description: string | null;
          event_date: string;
          financial_period_id: string | null;
          financial_product_id: string;
          id: string;
          is_confirmed: boolean;
          liability_installment_id: string | null;
          metadata: Json | null;
          notes: string | null;
          origin_type: string;
          priority: string | null;
          recurring_instance_id: string | null;
          source_document_id: string | null;
          statement_cycle_id: string | null;
          supplier_id: string | null;
          type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amount: number;
          category_id?: string | null;
          competence_date?: string | null;
          created_at?: string;
          description?: string | null;
          event_date: string;
          financial_period_id?: string | null;
          financial_product_id: string;
          id?: string;
          is_confirmed?: boolean;
          liability_installment_id?: string | null;
          metadata?: Json | null;
          notes?: string | null;
          origin_type?: string;
          priority?: string | null;
          recurring_instance_id?: string | null;
          source_document_id?: string | null;
          statement_cycle_id?: string | null;
          supplier_id?: string | null;
          type: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amount?: number;
          category_id?: string | null;
          competence_date?: string | null;
          created_at?: string;
          description?: string | null;
          event_date?: string;
          financial_period_id?: string | null;
          financial_product_id?: string;
          id?: string;
          is_confirmed?: boolean;
          liability_installment_id?: string | null;
          metadata?: Json | null;
          notes?: string | null;
          origin_type?: string;
          priority?: string | null;
          recurring_instance_id?: string | null;
          source_document_id?: string | null;
          statement_cycle_id?: string | null;
          supplier_id?: string | null;
          type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "transactions_category_id_fkey";
            columns: ["category_id"];
            isOneToOne: false;
            referencedRelation: "categories";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_financial_period_id_fkey";
            columns: ["financial_period_id"];
            isOneToOne: false;
            referencedRelation: "financial_periods";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_financial_product_id_fkey";
            columns: ["financial_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_liability_installment_id_fkey";
            columns: ["liability_installment_id"];
            isOneToOne: false;
            referencedRelation: "liability_installments";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_recurring_instance_id_fkey";
            columns: ["recurring_instance_id"];
            isOneToOne: false;
            referencedRelation: "recurring_instances";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_source_document_id_fkey";
            columns: ["source_document_id"];
            isOneToOne: false;
            referencedRelation: "source_documents";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_statement_cycle_id_fkey";
            columns: ["statement_cycle_id"];
            isOneToOne: false;
            referencedRelation: "statement_cycles";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transactions_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "mv_supplier_spending";
            referencedColumns: ["supplier_id"];
          },
          {
            foreignKeyName: "transactions_supplier_id_fkey";
            columns: ["supplier_id"];
            isOneToOne: false;
            referencedRelation: "suppliers";
            referencedColumns: ["id"];
          },
        ];
      };
      transfers: {
        Row: {
          amount: number;
          competence_date: string | null;
          created_at: string;
          description: string | null;
          event_date: string;
          financial_period_id: string | null;
          id: string;
          is_confirmed: boolean;
          notes: string | null;
          source_product_id: string;
          target_product_id: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          amount: number;
          competence_date?: string | null;
          created_at?: string;
          description?: string | null;
          event_date: string;
          financial_period_id?: string | null;
          id?: string;
          is_confirmed?: boolean;
          notes?: string | null;
          source_product_id: string;
          target_product_id: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          amount?: number;
          competence_date?: string | null;
          created_at?: string;
          description?: string | null;
          event_date?: string;
          financial_period_id?: string | null;
          id?: string;
          is_confirmed?: boolean;
          notes?: string | null;
          source_product_id?: string;
          target_product_id?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [
          {
            foreignKeyName: "transfers_financial_period_id_fkey";
            columns: ["financial_period_id"];
            isOneToOne: false;
            referencedRelation: "financial_periods";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transfers_source_product_id_fkey";
            columns: ["source_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
          {
            foreignKeyName: "transfers_target_product_id_fkey";
            columns: ["target_product_id"];
            isOneToOne: false;
            referencedRelation: "financial_products";
            referencedColumns: ["id"];
          },
        ];
      };
      user_financial_preferences: {
        Row: {
          created_at: string;
          default_currency: string;
          financial_cycle_anchor_date: string | null;
          financial_cycle_start_day: number | null;
          id: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          default_currency?: string;
          financial_cycle_anchor_date?: string | null;
          financial_cycle_start_day?: number | null;
          id?: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          default_currency?: string;
          financial_cycle_anchor_date?: string | null;
          financial_cycle_start_day?: number | null;
          id?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
      user_secrets: {
        Row: {
          created_at: string;
          encrypted_value: string;
          encryption_version: number;
          entity_id: string | null;
          entity_type: string | null;
          id: string;
          secret_type: string;
          updated_at: string;
          user_id: string;
        };
        Insert: {
          created_at?: string;
          encrypted_value: string;
          encryption_version?: number;
          entity_id?: string | null;
          entity_type?: string | null;
          id?: string;
          secret_type: string;
          updated_at?: string;
          user_id: string;
        };
        Update: {
          created_at?: string;
          encrypted_value?: string;
          encryption_version?: number;
          entity_id?: string | null;
          entity_type?: string | null;
          id?: string;
          secret_type?: string;
          updated_at?: string;
          user_id?: string;
        };
        Relationships: [];
      };
    };
    Views: {
      mv_supplier_spending: {
        Row: {
          first_transaction_date: string | null;
          last_transaction_date: string | null;
          periods_active: number | null;
          supplier_id: string | null;
          supplier_name: string | null;
          supplier_type: string | null;
          total_spent: number | null;
          transaction_count: number | null;
          user_id: string | null;
        };
        Relationships: [];
      };
      v_expenses_deduplicated: {
        Row: {
          amount: number | null;
          canonical_id: string | null;
          category_id: string | null;
          competence_date: string | null;
          description: string | null;
          event_date: string | null;
          financial_period_id: string | null;
          priority: string | null;
          source_type: string | null;
          statement_cycle_id: string | null;
          supplier_id: string | null;
          user_id: string | null;
        };
        Relationships: [];
      };
    };
    Functions: {
      confirm_supplier_associations: {
        Args: { p_confirmations: Json; p_user_id: string };
        Returns: Json;
      };
      decrypt_secret: { Args: { ciphertext: string }; Returns: string };
      encrypt_secret: { Args: { plaintext: string }; Returns: string };
      fn_reconciliation_progress: {
        Args: { p_batch_id: string };
        Returns: {
          progress_pct: number;
          reconciled_count: number;
          total_count: number;
        }[];
      };
      generate_financial_periods: {
        Args: {
          p_anchor_date?: string;
          p_months_ahead?: number;
          p_start_day: number;
        };
        Returns: number;
      };
      get_financial_period_for_date: {
        Args: { p_date: string };
        Returns: string;
      };
      increment_session_tokens: {
        Args: { p_messages?: number; p_session_id: string; p_tokens: number };
        Returns: undefined;
      };
      merge_suppliers: {
        Args: {
          p_source_id: string;
          p_source_name: string;
          p_target_id: string;
          p_target_name: string;
          p_user_id: string;
        };
        Returns: Json;
      };
      refresh_mv_supplier_spending: { Args: never; Returns: undefined };
      register_pattern_feedback: {
        Args: {
          p_corrections?: Json;
          p_feedback_type: string;
          p_pattern_id: string;
          p_source_document_id: string;
        };
        Returns: {
          corrections: Json;
          created_at: string;
          feedback_type: string;
          id: string;
          notes: string | null;
          pattern_id: string;
          source_document_id: string | null;
          user_id: string;
        };
        SetofOptions: {
          from: "*";
          to: "pattern_feedback";
          isOneToOne: true;
          isSetofReturn: false;
        };
      };
      search_suppliers: {
        Args: { p_limit?: number; p_query: string };
        Returns: {
          document_number: string;
          id: string;
          is_active: boolean;
          matched_alias: string;
          name: string;
          similarity_score: number;
          type: string;
        }[];
      };
      show_limit: { Args: never; Returns: number };
      show_trgm: { Args: { "": string }; Returns: string[] };
    };
    Enums: {
      [_ in never]: never;
    };
    CompositeTypes: {
      [_ in never]: never;
    };
  };
};

type DatabaseWithoutInternals = Omit<Database, "__InternalSupabase">;

type DefaultSchema = DatabaseWithoutInternals[Extract<keyof Database, "public">];

export type Tables<
  DefaultSchemaTableNameOrOptions extends
    | keyof (DefaultSchema["Tables"] & DefaultSchema["Views"])
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals;
  }
    ? keyof (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
        DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals;
}
  ? (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
      DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])[TableName] extends {
      Row: infer R;
    }
    ? R
    : never
  : DefaultSchemaTableNameOrOptions extends keyof (DefaultSchema["Tables"] & DefaultSchema["Views"])
    ? (DefaultSchema["Tables"] & DefaultSchema["Views"])[DefaultSchemaTableNameOrOptions] extends {
        Row: infer R;
      }
      ? R
      : never
    : never;

export type TablesInsert<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals;
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals;
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Insert: infer I;
    }
    ? I
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Insert: infer I;
      }
      ? I
      : never
    : never;

export type TablesUpdate<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals;
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals;
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Update: infer U;
    }
    ? U
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Update: infer U;
      }
      ? U
      : never
    : never;

export type Enums<
  DefaultSchemaEnumNameOrOptions extends
    | keyof DefaultSchema["Enums"]
    | { schema: keyof DatabaseWithoutInternals },
  EnumName extends DefaultSchemaEnumNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals;
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"]
    : never = never,
> = DefaultSchemaEnumNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals;
}
  ? DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"][EnumName]
  : DefaultSchemaEnumNameOrOptions extends keyof DefaultSchema["Enums"]
    ? DefaultSchema["Enums"][DefaultSchemaEnumNameOrOptions]
    : never;

export type CompositeTypes<
  PublicCompositeTypeNameOrOptions extends
    | keyof DefaultSchema["CompositeTypes"]
    | { schema: keyof DatabaseWithoutInternals },
  CompositeTypeName extends PublicCompositeTypeNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals;
  }
    ? keyof DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"]
    : never = never,
> = PublicCompositeTypeNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals;
}
  ? DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"][CompositeTypeName]
  : PublicCompositeTypeNameOrOptions extends keyof DefaultSchema["CompositeTypes"]
    ? DefaultSchema["CompositeTypes"][PublicCompositeTypeNameOrOptions]
    : never;

export const Constants = {
  graphql_public: {
    Enums: {},
  },
  public: {
    Enums: {},
  },
} as const;
