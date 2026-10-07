import type { GithubMergeMethod } from './githubMergeMethod';

export interface EnableAutoMergeRequest {
  /** The GitHub repository owner or organization. */
  owner: string;
  /** The GitHub repository name. */
  repo: string;
  /** The GitHub pull request number. */
  number: number;
  /** The merge method to use when auto-merging. */
  mergeMethod?: GithubMergeMethod;
}
