import type {
  CreateDestination,
  DestinationTaskComposer,
} from '@app/features/command/create-destination';
import type { Accessor } from 'solid-js';
import type { ProjectsContext } from '../context/projects-context';
import { canEditProject, type ProjectDetail } from '../core/project';

type CreateProjectTask = ReturnType<
  ProjectsContext['createCommands']
>['createTask'];

/**
 * Task composer props that add the new task to the project. The project's own
 * New task and the global create menu open the same composer with these.
 */
export function projectTaskComposer(
  project: Pick<ProjectDetail, 'id' | 'name'>,
  createTask: CreateProjectTask,
  onCreated: () => void
): DestinationTaskComposer {
  return {
    projectName: project.name,
    createTask: (...args) => createTask(project.id, ...args),
    onSuccess: onCreated,
  };
}

/**
 * Where the create menu puts a new task while this project is open. Withdrawn
 * until the project loads and for viewers who cannot add tasks to it, so their
 * tasks are created as they would be anywhere else.
 */
export function projectCreateDestination(
  project: Accessor<ProjectDetail | undefined>,
  createTask: CreateProjectTask,
  onCreated: () => void
): Accessor<CreateDestination | undefined> {
  return () => {
    const current = project();
    if (!current || !canEditProject(current)) return;
    return {
      label: current.name,
      taskComposer: projectTaskComposer(current, createTask, onCreated),
    };
  };
}
