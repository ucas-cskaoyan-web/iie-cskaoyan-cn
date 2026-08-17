import type { PageServerLoad } from './$types';
import { apiRequest } from '$server/api';
import type { AnnualReportDetail, AnnualReportOverview, Article, Category, PublicContributor } from '$lib/types';

export const load: PageServerLoad = async () => {
  const [report, reports, recent, categories, contributors] = await Promise.all([
    apiRequest<AnnualReportDetail>('/api/v1/reports/latest').catch(() => null),
    apiRequest<AnnualReportOverview[]>('/api/v1/reports').catch(() => []),
    apiRequest<Article[]>('/api/v1/articles?limit=4').catch(() => []),
    apiRequest<Category[]>('/api/v1/categories').catch(() => []),
    apiRequest<PublicContributor[]>('/api/v1/contributors').catch(() => [])
  ]);

  return { report, reports: reports.slice(0, 3), recent, categories, contributors };
};
